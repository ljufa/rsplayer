use futures::{Future, SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use rust_embed::RustEmbed;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
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
static ACTIVE_USERS: AtomicUsize = AtomicUsize::new(0);

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
    mut state_changes_rx: broadcast::Receiver<StateChangeEvent>,
    player_commands_tx: UserCommandSender,
    system_commands_tx: SystemCommandSender,
    config: &Config,
) -> (
    impl Future<Output = ()>,
    impl Future<Output = ()>,
    impl Future<Output = ()>,
) {
    let player_commands_tx = warp::any().map(move || player_commands_tx.clone());
    let system_commands_tx = warp::any().map(move || system_commands_tx.clone());
    let (ws_bcast_tx, _) = broadcast::channel::<Arc<String>>(32);
    let ws_bcast_tx_filter = ws_bcast_tx.clone();
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let player_ws_path = warp::path!("api" / "ws")
        .and(warp::ws())
        .and(warp::any().map(move || ws_bcast_tx_filter.subscribe()))
        .and(player_commands_tx)
        .and(system_commands_tx)
        .map(
            |ws: warp::ws::Ws, ws_rx, player_commands, system_commands| {
                ws.on_upgrade(|websocket| {
                    user_connected(websocket, ws_rx, player_commands, system_commands)
                })
            },
        );

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
    let ws_bcast_tx_handle = ws_bcast_tx;
    let ws_handle = async move {
        loop {
            match state_changes_rx.recv().await {
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    warn!("Websocket broadcaster lagged, skipped {count} messages.");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("State change event stream closed, exiting websocket handler.");
                    break;
                }
                Ok(ev) => {
                    trace!("Received state changed event {ev:?}");
                    let json_msg = serde_json::to_string(&ev).unwrap();
                    if !json_msg.is_empty()
                        && ws_bcast_tx_handle.send(Arc::new(json_msg)).is_err()
                    {
                        trace!("No active ws clients, not sending state change");
                    }
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

    pub fn settings_save(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::path!("api" / "settings"))
            .and(json_body())
            .and(with_config(config))
            .and(warp::query())
            .and_then(handlers::save_settings)
    }

    pub fn get_settings(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
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
            .map(move || error_msg.clone())
    }

    fn with_config(config: Config) -> impl Filter<Extract = (Config,), Error = std::convert::Infallible> + Clone
    {
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
    use rsplayer_hardware::{
        audio_device::alsa::{self},
        usb,
    };

    use super::Config;

    pub async fn save_settings(
        settings: Settings,
        config: Config,
        query: HashMap<String, String>,
    ) -> Result<impl warp::Reply, Infallible> {
        debug!("Settings to save {settings:?} and reload {query:?}");
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
        let mut settings = config.get_settings_mut();
        let cards = alsa::get_all_cards();
        if let Some(mixer_name) = &settings.volume_ctrl_settings.alsa_mixer_name {
            for card in &cards {
                if let Some(mixer) = card.mixers.iter().find(|m| &m.name == mixer_name) {
                    settings.volume_ctrl_settings.alsa_mixer = Some(mixer.clone());
                    break;
                }
            }
        }
        settings.alsa_settings.available_audio_cards = cards;

        Ok(warp::reply::json(&*settings))
    }
}

async fn user_connected(
    ws: WebSocket,
    mut ws_rx: broadcast::Receiver<Arc<String>>,
    user_commands_tx: UserCommandSender,
    system_commands_tx: SystemCommandSender,
) {
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    debug!("new websocket client: {user_id}");
    let current_users = ACTIVE_USERS.fetch_add(1, Ordering::SeqCst) + 1;
    info!("Number of active websockets is: {current_users}");

    let (mut to_user_ws, mut from_user_ws) = ws.split();

    loop {
        tokio::select! {
            // Receive message from client
            Some(result) = from_user_ws.next() => {
                let msg = match result {
                    Ok(msg) => msg,
                    Err(e) => {
                        debug!("websocket error(uid={user_id}): {e}");
                        break;
                    }
                };
                if msg.is_close() {
                    break;
                }
                if let Ok(cmd) = msg.to_str() {
                    if cmd.is_empty() {
                        continue;
                    }
                    info!("Got command from user {user_id}: {cmd:?}");
                    let user_command: Option<UserCommand> = serde_json::from_str(cmd).ok();
                    if let Some(pc) = user_command {
                        if user_commands_tx.send(pc).await.is_err() {
                            error!("failed to send user message");
                            break;
                        }
                    } else {
                        let system_command: Option<SystemCommand> = serde_json::from_str(cmd).ok();
                        if let Some(sc) = system_command {
                            if system_commands_tx.send(sc).await.is_err() {
                                error!("failed to send system message");
                                break;
                            }
                        } else {
                            warn!("Unknown command received: [{cmd}]");
                        }
                    }
                }
            },
            // Send broadcast message to client
            result = ws_rx.recv() => {
                match result {
                    Ok(json_msg) => {
                        if to_user_ws
                            .send(Message::text(json_msg.as_ref().clone()))
                            .await
                            .is_err()
                        {
                            debug!("Failed to send message to user {user_id}, client disconnected.");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client {user_id} is lagging, skipped {n} messages.");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    user_disconnected(user_id);
}

fn user_disconnected(my_id: usize) {
    info!("good bye user: {my_id}");
    let current_users = ACTIVE_USERS.fetch_sub(1, Ordering::SeqCst) - 1;
    info!("Number of active websockets is: {current_users}");
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
