use futures::Future;
use futures::FutureExt;
use futures::StreamExt;

use crate::config::Configuration;
use crate::player::player_service::PlayerService;
use crate::player::spotify_oauth::SpotifyOauth;

use api_models::common::Command;
use api_models::state::{LastState, StateChangeEvent};
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
type LastStatusMessages = Arc<RwLock<LastState>>;
type Config = Arc<Mutex<Configuration>>;
type PlayerServiceArc = Arc<Mutex<PlayerService>>;
type SpotifyOauthArc = Arc<Mutex<SpotifyOauth>>;
type SyncSender = tokio::sync::mpsc::Sender<Command>;

pub fn start_degraded(config: Config) -> impl Future<Output = ()> {
    let spotify_oauth = Arc::new(Mutex::new(SpotifyOauth::new(
        config.lock().unwrap().get_settings().spotify_settings,
    )));

    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let ui_static_content = warp::get().and(warp::fs::dir(Configuration::get_static_dir_path()));

    let routes = filters::settings_save(config.clone())
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config))
        .or(filters::get_spotify_authorization_url(
            spotify_oauth.clone(),
        ))
        .or(filters::is_spotify_authorization_completed(
            spotify_oauth.clone(),
        ))
        .or(filters::spotify_authorization_callback(
            spotify_oauth.clone(),
        ))
        .or(filters::get_spotify_account_info(spotify_oauth))
        .or(ui_static_content)
        .with(cors);
    warp::serve(routes).run(([0, 0, 0, 0], 8000))
}

pub fn start(
    mut state_changes_rx: Receiver<StateChangeEvent>,
    input_commands_tx: SyncSender,
    config: Config,
    player_service: PlayerServiceArc,
) -> (impl Future<Output = ()>, impl Future<Output = ()>) {
    let spotify_oauth = Arc::new(Mutex::new(SpotifyOauth::new(
        config.lock().unwrap().get_settings().spotify_settings,
    )));

    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let users = Users::default();
    // Turn our "state" into a new Filter...
    let users_notify = users.clone();
    let users_f = warp::any().map(move || users.clone());
    let input_commands_tx = warp::any().map(move || input_commands_tx.clone());
    let last_state_change_message: LastStatusMessages = LastStatusMessages::default();
    let last_state_change_message_2 = last_state_change_message.clone();
    let last_state_change_message_3 = last_state_change_message.clone();
    let last_state_change_message_f = warp::any().map(move || last_state_change_message.clone());
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let player_ws_path = warp::path!("api" / "ws")
        .and(warp::ws())
        .and(users_f)
        .and(input_commands_tx)
        .and(last_state_change_message_f)
        .map(
            |ws: warp::ws::Ws, users, input_commands, last_state_change_message| {
                // And then our closure will be called when it completes...
                ws.on_upgrade(|websocket| {
                    user_connected(websocket, users, input_commands, last_state_change_message)
                })
            },
        );
    let ui_static_content = warp::get().and(warp::fs::dir(Configuration::get_static_dir_path()));

    let routes = player_ws_path
        .or(filters::settings_save(config.clone()))
        .or(filters::get_settings(config))
        .or(filters::get_playlists(player_service.clone()))
        .or(filters::get_queue_items(player_service.clone()))
        .or(filters::get_spotify_authorization_url(
            spotify_oauth.clone(),
        ))
        .or(filters::is_spotify_authorization_completed(
            spotify_oauth.clone(),
        ))
        .or(filters::spotify_authorization_callback(
            spotify_oauth.clone(),
        ))
        .or(filters::get_spotify_account_info(spotify_oauth))
        .or(filters::get_playlist_items(player_service))
        .or(filters::get_spotify_authorization_url(
            spotify_oauth.clone(),
        ))
        .or(filters::is_spotify_authorization_completed(
            spotify_oauth.clone(),
        ))
        .or(filters::spotify_authorization_callback(
            spotify_oauth.clone(),
        ))
        .or(filters::get_spotify_account_info(spotify_oauth))
        .or(filters::get_last_status(last_state_change_message_3))
        .or(ui_static_content)
        .with(cors);

    let ws_handle = async move {
        loop {
            match state_changes_rx.try_recv() {
                Err(_e) => {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Ok(ev) => {
                    trace!("Received state changed event {:?}", ev);
                    notify_users(&users_notify, ev, last_state_change_message_2.clone()).await;
                }
            }
        }
    };
    let http_handle = warp::serve(routes).run(([0, 0, 0, 0], 8000));
    (http_handle, ws_handle)
}
mod filters {
    use std::collections::HashMap;

    use api_models::settings::Settings;
    use warp::Filter;

    use super::{handlers, Config, LastStatusMessages, PlayerServiceArc, SpotifyOauthArc};

    pub fn settings_save(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::path!("api" / "settings"))
            .and(json_body())
            .and(with_config(config))
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
    pub fn get_playlists(
        player_service: PlayerServiceArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "playlist"))
            .and(with_player_svc(player_service))
            .and_then(handlers::get_playlists)
    }

    pub fn get_queue_items(
        player_service: PlayerServiceArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "queue"))
            .and(with_player_svc(player_service))
            .and_then(handlers::get_queue_items)
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
        spotify_oauth: SpotifyOauthArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "get-url"))
            .and(with_spotify_oauth(spotify_oauth))
            .and_then(handlers::get_spotify_authorization_url)
    }
    pub fn is_spotify_authorization_completed(
        spotify_oauth: SpotifyOauthArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "is-authorized"))
            .and(with_spotify_oauth(spotify_oauth))
            .and_then(handlers::is_spotify_authorization_completed)
    }
    pub fn spotify_authorization_callback(
        spotify_oauth: SpotifyOauthArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "callback"))
            .and(warp::query::<HashMap<String, String>>())
            .and(with_spotify_oauth(spotify_oauth))
            .and_then(handlers::spotify_authorization_callback)
    }
    pub fn get_spotify_account_info(
        spotify_oauth: SpotifyOauthArc,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "spotify" / "me"))
            .and(with_spotify_oauth(spotify_oauth))
            .and_then(handlers::get_spotify_account_info)
    }

    pub fn get_last_status(
        last_state_message: LastStatusMessages,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::get()
            .and(warp::path!("api" / "status"))
            .and(with_status_messages(last_state_message))
            .and_then(handlers::get_last_status)
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
    fn with_spotify_oauth(
        spotify_oauth: SpotifyOauthArc,
    ) -> impl Filter<Extract = (SpotifyOauthArc,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || spotify_oauth.clone())
    }

    fn with_status_messages(
        last_state_message: LastStatusMessages,
    ) -> impl Filter<Extract = (LastStatusMessages,), Error = std::convert::Infallible> + Clone
    {
        warp::any().map(move || last_state_message.clone())
    }

    fn json_body() -> impl Filter<Extract = (Settings,), Error = warp::Rejection> + Clone {
        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        warp::body::json()
    }
}

mod handlers {
    use std::{collections::HashMap, convert::Infallible, ops::Deref};

    use super::{Config, LastStatusMessages, PlayerServiceArc, SpotifyOauthArc};
    use api_models::settings::Settings;

    use api_models::state::LastState;
    use warp::hyper::StatusCode;

    pub async fn save_settings(
        settings: Settings,
        config: Config,
    ) -> Result<impl warp::Reply, Infallible> {
        debug!("Settings to save {:?}", settings);
        config.lock().unwrap().save_settings(&settings);
        // todo: find better way to trigger service restart by systemd
        match std::process::Command::new("sudo")
            .arg("systemctl")
            .arg("restart")
            .arg("dplay")
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
    }

    pub async fn get_settings(config: Config) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(&config.lock().unwrap().get_settings()))
    }
    pub async fn get_playlists(
        player_service: PlayerServiceArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &player_service
                .lock()
                .unwrap()
                .get_current_player()
                .get_playlists(),
        ))
    }

    pub async fn get_spotify_authorization_url(
        spotify_oauth: SpotifyOauthArc,
    ) -> Result<impl warp::Reply, Infallible> {
        match &spotify_oauth.lock().unwrap().get_authorization_url() {
            Ok(url) => Ok(warp::reply::with_status(url.clone(), StatusCode::OK)),
            Err(e) => Ok(warp::reply::with_status(
                e.to_string(),
                StatusCode::BAD_GATEWAY,
            )),
        }
    }
    pub async fn is_spotify_authorization_completed(
        spotify_oauth: SpotifyOauthArc,
    ) -> Result<impl warp::Reply, Infallible> {
        match &spotify_oauth.lock().unwrap().is_token_present() {
            Ok(auth) => Ok(warp::reply::with_status(auth.to_string(), StatusCode::OK)),
            Err(e) => Ok(warp::reply::with_status(
                e.to_string(),
                StatusCode::BAD_GATEWAY,
            )),
        }
    }
    pub async fn spotify_authorization_callback(
        url: HashMap<String, String>,
        spotify_oauth: SpotifyOauthArc,
    ) -> Result<impl warp::Reply, Infallible> {
        match &spotify_oauth
            .lock()
            .unwrap()
            .authorize_callback(url.get("code").unwrap())
        {
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

    pub async fn get_spotify_account_info(
        spotify_oauth: SpotifyOauthArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &spotify_oauth.lock().unwrap().get_account_info(),
        ))
    }

    pub async fn get_queue_items(
        player_service: PlayerServiceArc,
    ) -> Result<impl warp::Reply, Infallible> {
        Ok(warp::reply::json(
            &player_service
                .lock()
                .unwrap()
                .get_current_player()
                .get_queue_items(),
        ))
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
    pub async fn get_last_status(
        last_state_message: LastStatusMessages,
    ) -> Result<impl warp::Reply, Infallible> {
        if let Ok(m) = last_state_message.try_read() {
            return Ok(warp::reply::json(&m.deref()));
        }
        Ok(warp::reply::json(&LastState::default()))
    }
}

async fn notify_users(
    users_to_notify: &Users,
    status_change_event: StateChangeEvent,
    last_state_change_message_2: LastStatusMessages,
) {
    let json_msg = serde_json::to_string(&status_change_event).unwrap();
    match status_change_event {
        StateChangeEvent::CurrentTrackInfoChanged(t) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.current_track_info = Some(t);
            }
        }
        StateChangeEvent::StreamerStateChanged(s) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.streamer_state = Some(s);
            }
        }
        StateChangeEvent::PlayerInfoChanged(p) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.player_info = Some(p);
            }
        }
        _ => {}
    }
    if !json_msg.is_empty() {
        for (&_uid, tx) in users_to_notify.read().await.iter() {
            tx.send(Ok(Message::text(json_msg.clone()))).unwrap();
        }
    }
}

async fn user_connected(
    ws: WebSocket,
    users: Users,
    input_commands_tx: SyncSender,
    last_state_message: LastStatusMessages,
) {
    // Use a counter to assign a new unique ID for this user.
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    debug!("new websocket client: {}", my_id);

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

    if let Ok(last) = last_state_message.try_read() {
        if let Some(csi) = &last.current_track_info {
            let json =
                serde_json::to_string(&StateChangeEvent::CurrentTrackInfoChanged(csi.clone()))
                    .unwrap_or_default();
            tx.send(Ok(Message::text(json)))
                .expect("Send message failed");
        }
        if let Some(pi) = &last.player_info {
            let json = serde_json::to_string(&StateChangeEvent::PlayerInfoChanged(pi.clone()))
                .unwrap_or_default();
            tx.send(Ok(Message::text(json)))
                .expect("Send message failed");
        }
        if let Some(ss) = &last.streamer_state {
            let json = serde_json::to_string(&StateChangeEvent::StreamerStateChanged(ss.clone()))
                .unwrap_or_default();
            tx.send(Ok(Message::text(json)))
                .expect("Send message failed");
        }
    }
    // Save the sender in our list of connected users.
    users.write().await.insert(my_id, tx);

    // input socket loop
    while let Some(result) = from_user_ws.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                debug!("websocket error(uid={}): {}", my_id, e);
                break;
            }
        };
        info!("Got command from user {:?}", msg);
        if let Ok(cmd) = msg.to_str() {
            let cmd: Command = serde_json::from_str(cmd).unwrap();
            _ = input_commands_tx.send(cmd).await;
        }
    }

    // Make an extra clone to give to our disconnection handler...
    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(my_id, &users.clone()).await;
}

async fn user_disconnected(my_id: usize, users: &Users) {
    debug!("good bye user: {}", my_id);

    // Stream closed up, so remove from the user list
    users.write().await.remove(&my_id);
}
