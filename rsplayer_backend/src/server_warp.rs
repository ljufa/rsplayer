use std::env;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::{sync::Arc, time::Duration};

use futures::Future;
use futures::FutureExt;
use futures::StreamExt;
use log::debug;
use log::info;
use log::warn;
use rust_embed::RustEmbed;
use tokio::{
    sync::{broadcast::Receiver, mpsc, RwLock},
    time::sleep,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::http::{HeaderMap, HeaderValue};
use warp::{
    hyper::Method,
    ws::{Message, WebSocket},
    Filter,
};

use api_models::common::SystemCommand;
use api_models::common::UserCommand;
use api_models::serde_json;
use api_models::state::StateChangeEvent;
use rsplayer_config::Configuration;

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Result<Message, warp::Error>>>>>;
type Config = Arc<Configuration>;

type UserCommandSender = mpsc::Sender<UserCommand>;
type SystemCommandSender = mpsc::Sender<SystemCommand>;

#[derive(RustEmbed)]
#[folder = "../rsplayer_web_ui/public"]
struct StaticContentDir;

pub fn start_degraded(config: &Config, error: &anyhow::Error) -> impl Future<Output = ()> {
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let ui_static_content = warp::get().and(warp_embed::embed(&StaticContentDir));

    let routes = filters::settings_save(config.clone())
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config.clone()))
        .or(ui_static_content)
        .or(filters::get_startup_error(error))
        .with(cors);

    warp::serve(routes).run(([0, 0, 0, 0], get_ports().0))
}

pub fn start(
    mut state_changes_rx: Receiver<StateChangeEvent>,
    player_commands_tx: UserCommandSender,
    system_commands_tx: SystemCommandSender,
    config: &Config,
) -> (
    impl Future<Output = ()>,
    impl Future<Output = ()>,
    impl Future<Output = ()>,
) {
    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let users = Users::default();
    // Turn our "state" into a new Filter...
    let users_notify = users.clone();
    let users_f = warp::any().map(move || users.clone());
    let player_commands_tx = warp::any().map(move || player_commands_tx.clone());
    let system_commands_tx = warp::any().map(move || system_commands_tx.clone());
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let player_ws_path = warp::path!("api" / "ws")
        .and(warp::ws())
        .and(users_f)
        .and(player_commands_tx)
        .and(system_commands_tx)
        .map(|ws: warp::ws::Ws, users, player_commands, system_commands| {
            // And then our closure will be called when it completes...
            ws.on_upgrade(|websocket| user_connected(websocket, users, player_commands, system_commands))
        });

    let mut cache_headers = HeaderMap::new();
    cache_headers.insert(
        warp::http::header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=259200"), // 3 days
    );

    let ui_static_content = warp::get()
        .and(warp_embed::embed(&StaticContentDir))
        .with(warp::compression::gzip())
        .with(warp::reply::with::headers(cache_headers.clone()));
    let artwork_static_content = warp::path("artwork")
        .and(warp::fs::dir("artwork"))
        .with(warp::compression::gzip())
        .with(warp::reply::with::headers(cache_headers));

    let routes = player_ws_path
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config.clone()))
        .or(ui_static_content)
        .or(artwork_static_content)
        .with(cors);

    let ws_handle = async move {
        loop {
            match state_changes_rx.recv().await {
                Err(_e) => {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Ok(ev) => {
                    debug!("Received state changed event {:?}", ev);
                    notify_users(&users_notify, ev).await;
                }
            }
        }
    };
    let ports = get_ports();
    let http_handle = warp::serve(routes.clone()).run(([0, 0, 0, 0], ports.0));
    let cert_path = env::var("TLS_CERT_PATH").expect("TLS_CERT_PATH is not set");
    let key_path = env::var("TLS_CERT_KEY_PATH").expect("TLS_CERT_KEY_PATH is not set");
    let https_handle = warp::serve(routes)
        .tls()
        .cert_path(cert_path)
        .key_path(key_path)
        .run(([0, 0, 0, 0], ports.1));
    (http_handle, https_handle, ws_handle)
}

#[allow(warnings)]
mod filters {
    use warp::Filter;

    use api_models::settings::Settings;

    use super::{handlers, Config};

    pub fn settings_save(config: Config) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::path!("api" / "settings"))
            .and(json_body())
            .and(with_config(config))
            .and(warp::query())
            .and_then(handlers::save_settings)
    }

    pub fn get_settings(config: Config) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "settings"))
            .and(with_config(config))
            .and_then(handlers::get_settings)
    }

    pub fn get_startup_error(
        error: &anyhow::Error,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let error_msg = error.to_string();
        warp::get()
            .and(warp::path!("api" / "start_error"))
            .map(move || error_msg.to_string())
    }

    fn with_config(config: Config) -> impl Filter<Extract = (Config,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || config.clone())
    }

    fn json_body() -> impl Filter<Extract = (Settings,), Error = warp::Rejection> + Clone {
        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        warp::body::json()
    }
}

#[allow(warnings, clippy::unused_async)]
mod handlers {
    use std::{collections::HashMap, convert::Infallible, process::exit};

    use log::{debug, error};
    use warp::hyper::StatusCode;

    use api_models::settings::Settings;
    use rsplayer_hardware::audio_device::alsa::{self};

    use super::Config;

    pub async fn save_settings(
        settings: Settings,
        config: Config,
        query: HashMap<String, String>,
    ) -> Result<impl warp::Reply, Infallible> {
        debug!("Settings to save {:?} and reload {:?}", settings, query);
        config.save_settings(&settings);
        let param = query.get("reload").unwrap();
        if param == "true" {
            info!("Reloading service");
            // systemd should start the service again
            exit(1);
        } else {
            Ok(StatusCode::CREATED)
        }
    }

    pub async fn get_settings(config: Config) -> Result<impl warp::Reply, Infallible> {
        let settings = &mut config.get_settings();
        let cards = alsa::get_all_cards();

        settings.alsa_settings.available_audio_cards = cards;

        Ok(warp::reply::json(settings))
    }
}

async fn notify_users(users_to_notify: &Users, status_change_event: StateChangeEvent) {
    if !users_to_notify.read().await.is_empty() {
        let json_msg = serde_json::to_string(&status_change_event).unwrap();
        if !json_msg.is_empty() {
            let users = users_to_notify.read().await;
            users.iter().for_each(|tx| {
                let send_result = tx.1.send(Ok(Message::text(json_msg.clone())));
                debug!("Sent message to user: {:?} with result: {:?}", tx.0, send_result);
            });
        }
    }
}

async fn user_connected(
    ws: WebSocket,
    users: Users,
    user_commands_tx: UserCommandSender,
    system_commands_tx: SystemCommandSender,
) {
    // Use a counter to assign a new unique ID for this user.
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    debug!("new websocket client: {}", user_id);

    // Split the socket into a sender and receive of messages.
    let (to_user_ws, mut from_user_ws) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    let rx = UnboundedReceiverStream::new(rx);
    _ = tokio::task::Builder::new()
        .name(&format!("Websocket thread for user:{user_id}"))
        .spawn(rx.forward(to_user_ws).map(|result| {
            if let Err(e) = result {
                debug!("websocket send error: {}", e);
            }
        }));

    // Save the sender in our list of connected users.
    users.write().await.insert(user_id, tx);

    // input socket loop
    while let Some(result) = from_user_ws.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                debug!("websocket error(uid={}): {}", user_id, e);
                break;
            }
        };
        info!("Got command from user {:?}", msg);
        if let Ok(cmd) = msg.to_str() {
            let user_command: Option<UserCommand> = serde_json::from_str(cmd).ok();
            if let Some(pc) = user_command {
                user_commands_tx.send(pc).await.expect("failed to send user message");
            } else {
                let system_command: Option<SystemCommand> = serde_json::from_str(cmd).ok();
                if let Some(sc) = system_command {
                    system_commands_tx
                        .send(sc)
                        .await
                        .expect("failed to send system message");
                } else {
                    warn!("Unknown command received: [{}]", cmd);
                }
            }
        }
    }

    // Make an extra clone to give to our disconnection handler...
    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(user_id, &users.clone()).await;
}

async fn user_disconnected(my_id: usize, users: &Users) {
    info!("good bye user: {}", my_id);
    // Stream closed up, so remove from the user list
    users.write().await.remove(&my_id);
    info!("Number of active websockets is: {}", users.read().await.len());
}

fn get_ports() -> (u16, u16) {
    let http_port = env::var("PORT")
        .expect("PORT is not set")
        .parse::<u16>()
        .expect("PORT is not a valid port number");

    let https_port = env::var("TLS_PORT")
        .expect("TLS_PORT is not set")
        .parse::<u16>()
        .expect("TLS_PORT is not a valid port number");
    (http_port, https_port)
}
