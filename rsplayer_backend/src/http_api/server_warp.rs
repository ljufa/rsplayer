use api_models::common::SystemCommand;
use api_models::serde_json;
use futures::Future;
use futures::FutureExt;
use futures::StreamExt;

use crate::config::Configuration;
use crate::player::player_service::PlayerService;

use api_models::common::PlayerCommand;
use api_models::state::StateChangeEvent;
use std::env;
use std::net::Ipv4Addr;
use std::net::SocketAddrV4;
use std::net::TcpListener;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{broadcast::Receiver, mpsc, RwLock},
    time::sleep,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::{
    hyper::Method,
    ws::{Message, WebSocket},
    Filter,
};

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Result<Message, warp::Error>>>>>;
type Config = Arc<Mutex<Configuration>>;
type PlayerServiceArc = Arc<Mutex<PlayerService>>;
type PlayerCommandSender = tokio::sync::mpsc::Sender<PlayerCommand>;
type SystemCommandSender = tokio::sync::mpsc::Sender<SystemCommand>;

pub fn start_degraded(config: &Config, error: &failure::Error) -> impl Future<Output = ()> {
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let ui_static_content = warp::get().and(warp::fs::dir(Configuration::get_static_dir_path()));

    let routes = filters::settings_save(config.clone())
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config.clone()))
        .or(filters::get_spotify_authorization_url())
        .or(filters::is_spotify_authorization_completed())
        .or(filters::spotify_authorization_callback())
        .or(filters::get_spotify_account_info())
        .or(ui_static_content)
        .or(filters::get_startup_error(error))
        .with(cors);

    warp::serve(routes).run(([0, 0, 0, 0], get_port()))
}

pub fn start(
    mut state_changes_rx: Receiver<StateChangeEvent>,
    player_commands_tx: PlayerCommandSender,
    system_commands_tx: SystemCommandSender,
    config: &Config,
    player_service: PlayerServiceArc,
) -> (impl Future<Output = ()>, impl Future<Output = ()>) {
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
        .map(
            |ws: warp::ws::Ws, users, player_commands, system_commands| {
                // And then our closure will be called when it completes...
                ws.on_upgrade(|websocket| {
                    user_connected(websocket, users, player_commands, system_commands)
                })
            },
        );
    let ui_static_content = warp::get()
        .and(warp::fs::dir(Configuration::get_static_dir_path()))
        .with(warp::compression::gzip());

    let routes = player_ws_path
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config.clone()))
        .or(filters::get_static_playlists(player_service.clone()))
        .or(filters::get_playlist_items(player_service.clone()))
        .or(filters::get_playlist_categories(player_service))
        .or(filters::get_spotify_authorization_url())
        .or(filters::is_spotify_authorization_completed())
        .or(filters::spotify_authorization_callback())
        .or(filters::get_spotify_account_info())
        .or(ui_static_content)
        .with(cors);

    let ws_handle = async move {
        loop {
            match state_changes_rx.recv().await {
                Err(_e) => {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Ok(ev) => {
                    trace!("Received state changed event {:?}", ev);
                    notify_users(&users_notify, ev).await;
                }
            }
        }
    };
    let http_handle = warp::serve(routes).run(([0, 0, 0, 0], get_port()));
    (http_handle, ws_handle)
}
mod filters {
    use std::collections::HashMap;

    use api_models::settings::Settings;
    use warp::Filter;

    use super::{handlers, Config, PlayerServiceArc};

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
        error: &failure::Error,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let error_msg = error.to_string();
        warp::get()
            .and(warp::path!("api" / "start_error"))
            .map(move || error_msg.to_string())
    }
    pub fn get_static_playlists(
        player_service: PlayerServiceArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "playlist"))
            .and(with_player_svc(player_service))
            .and_then(handlers::get_static_playlists)
    }

    pub fn get_playlist_categories(
        player_service: PlayerServiceArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "categories"))
            .and(with_player_svc(player_service))
            .and_then(handlers::get_playlist_categories)
    }

    pub fn get_playlist_items(
        player_service: PlayerServiceArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "playlist" / String))
            .and(with_player_svc(player_service))
            .and_then(handlers::get_playlist_items)
    }
    pub fn get_spotify_authorization_url(
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "get-url"))
            .and_then(handlers::get_spotify_authorization_url)
    }
    pub fn is_spotify_authorization_completed(
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "is-authorized"))
            .and_then(handlers::is_spotify_authorization_completed)
    }
    pub fn spotify_authorization_callback(
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "callback"))
            .and(warp::query::<HashMap<String, String>>())
            .and_then(handlers::spotify_authorization_callback)
    }
    pub fn get_spotify_account_info(
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "me"))
            .and_then(handlers::get_spotify_account_info)
    }

    fn with_config(
        config: Config,
    ) -> impl Filter<Extract = (Config,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || config.clone())
    }

    fn with_player_svc(
        player_svc: PlayerServiceArc,
    ) -> impl Filter<Extract = (PlayerServiceArc,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || player_svc.clone())
    }

    fn json_body() -> impl Filter<Extract = (Settings,), Error = warp::Rejection> + Clone {
        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        warp::body::json()
    }
}

mod handlers {
    use std::{collections::HashMap, convert::Infallible};

    use crate::{config, player::spotify_oauth::SpotifyOauth};

    use super::{Config, PlayerServiceArc};
    use api_models::settings::Settings;

    use warp::hyper::StatusCode;

    pub async fn save_settings(
        settings: Settings,
        config: Config,
        query: HashMap<String, String>,
    ) -> Result<impl warp::Reply, Infallible> {
        debug!("Settings to save {:?} and reload {:?}", settings, query);
        config.lock().unwrap().save_settings(&settings);
        // todo: find better way to trigger service restart by systemd
        let param = query.get("reload").unwrap();

        if param == "true" {
            match std::process::Command::new("sudo")
                .arg("systemctl")
                .arg("restart")
                .arg("rsplayer")
                .spawn()
            {
                Ok(child) => {
                    debug!("Restart command invoked.");
                    child.wait_with_output().expect("Error");
                    Ok(StatusCode::CREATED)
                }
                Err(e) => {
                    error!("Restart command failed with error:{e}");
                    Ok(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        } else {
            Ok(StatusCode::CREATED)
        }
    }

    pub async fn get_settings(config: Config) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(&config.lock().unwrap().get_settings()))
    }

    pub async fn get_static_playlists(
        player_service: PlayerServiceArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &player_service
                .lock()
                .unwrap()
                .get_current_player()
                .get_static_playlists(),
        ))
    }
    pub async fn get_playlist_categories(
        player_service: PlayerServiceArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &player_service
                .lock()
                .unwrap()
                .get_current_player()
                .get_playlist_categories(),
        ))
    }

    pub async fn get_spotify_authorization_url() -> Result<impl warp::Reply, Infallible> {
        let mut spotify_oauth =
            SpotifyOauth::new(&config::Configuration::new().get_settings().spotify_settings);
        match &spotify_oauth.get_authorization_url() {
            Ok(url) => Ok(warp::reply::with_status(url.clone(), StatusCode::OK)),
            Err(e) => Ok(warp::reply::with_status(
                e.to_string(),
                StatusCode::BAD_GATEWAY,
            )),
        }
    }
    pub async fn is_spotify_authorization_completed() -> Result<impl warp::Reply, Infallible> {
        let mut spotify_oauth =
            SpotifyOauth::new(&config::Configuration::new().get_settings().spotify_settings);
        match &spotify_oauth.is_token_present() {
            Ok(auth) => Ok(warp::reply::with_status(auth.to_string(), StatusCode::OK)),
            Err(e) => Ok(warp::reply::with_status(
                e.to_string(),
                StatusCode::BAD_GATEWAY,
            )),
        }
    }
    pub async fn spotify_authorization_callback(
        url: HashMap<String, String>,
    ) -> Result<impl warp::Reply, Infallible> {
        let mut spotify_oauth =
            SpotifyOauth::new(&config::Configuration::new().get_settings().spotify_settings);
        match &spotify_oauth.authorize_callback(url.get("code").unwrap()) {
            Ok(_) => Ok(warp::reply::html(
                r#"<html>
                            <body>
                            <div>
                                <p>Success!</p>
                                <button onclick='self.close()'>Close</button>
                            </div>
                            </body>
                        </html>"#,
            )),
            Err(e) => {
                error!("Authorization callback error:{}", e);
                Ok(warp::reply::html(r#"<p>Error</p>"#))
            }
        }
    }

    pub async fn get_spotify_account_info() -> Result<impl warp::Reply, Infallible> {
        let mut spotify_oauth =
            SpotifyOauth::new(&config::Configuration::new().get_settings().spotify_settings);
        Ok(warp::reply::json(&spotify_oauth.get_account_info()))
    }

    pub async fn get_playlist_items(
        playlist_name: String,
        player_service: PlayerServiceArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &player_service
                .lock()
                .unwrap()
                .get_current_player()
                .get_playlist_items(playlist_name),
        ))
    }
}

async fn notify_users(users_to_notify: &Users, status_change_event: StateChangeEvent) {
    if !users_to_notify.read().await.is_empty() {
        let json_msg = serde_json::to_string(&status_change_event).unwrap();
        if !json_msg.is_empty() {
            for (&_uid, tx) in users_to_notify.read().await.iter() {
                _ = tx.send(Ok(Message::text(json_msg.clone())));
            }
        }
    }
}

async fn user_connected(
    ws: WebSocket,
    users: Users,
    player_commands_tx: PlayerCommandSender,
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
    tokio::task::spawn(rx.forward(to_user_ws).map(|result| {
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
            let player_command: Option<PlayerCommand> = serde_json::from_str(cmd).ok();
            if let Some(pc) = player_command {
                _ = player_commands_tx.send(pc).await;
            } else {
                let system_command: Option<SystemCommand> = serde_json::from_str(cmd).ok();
                if let Some(sc) = system_command {
                    _ = system_commands_tx.send(sc).await;
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
    debug!("good bye user: {}", my_id);

    // Stream closed up, so remove from the user list
    users.write().await.remove(&my_id);
}

fn get_port() -> u16 {
    let fallback_port = 8000;
    let default_port = "80";
    let port = env::var("RSPLAYER_HTTP_PORT").unwrap_or_else(|_| default_port.to_string());
    if let Ok(port) = port.parse::<u16>() {
        if is_local_port_free(port) {
            return port;
        }
        warn!(
            "Desired port {} is unavailable, will try fallback port {}",
            port, fallback_port
        );

        if is_local_port_free(fallback_port) {
            return fallback_port;
        }
        error!("Fallback port {} is also unavailable", fallback_port);
    }
    panic!("Desired port [{}], default port [{}] and fallback port [{}] are unavailable! Please specify another value for RSPLAYER_HTTP_PORT in rsplayer.service file", port, default_port, fallback_port)
}

fn is_local_port_free(port: u16) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    TcpListener::bind(ipv4).is_ok()
}
