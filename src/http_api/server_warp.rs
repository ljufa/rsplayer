use futures::Future;
use futures::FutureExt;
use futures::StreamExt;

use api_models::player::*;
use tokio::task::JoinHandle;
use warp::Server;

use crate::config::Configuration;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::SyncSender,
    },
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
type LastStatusMessages = Arc<RwLock<LastStatus>>;
type Config = Arc<Mutex<Configuration>>;

struct LastStatus {
    current_track_info: Option<String>,
    player_info: Option<String>,
    streamer_status: Option<String>,
}
impl Default for LastStatus {
    fn default() -> Self {
        LastStatus {
            current_track_info: None,
            player_info: None,
            streamer_status: None,
        }
    }
}
pub fn start(
    mut state_changes_receiver: Receiver<StatusChangeEvent>,
    input_commands_tx: SyncSender<Command>,
    config: Config,
) -> (impl Future<Output = ()>, JoinHandle<()>) {
    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let users = Users::default();
    // Turn our "state" into a new Filter...
    let users_notify = users.clone();
    let users_f = warp::any().map(move || users.clone());
    let input_commands_tx = warp::any().map(move || input_commands_tx.clone());
    let last_state_change_message: LastStatusMessages = LastStatusMessages::default();
    let last_state_change_message_2 = last_state_change_message.clone();
    let last_state_change_message_f = warp::any().map(move || last_state_change_message.clone());
    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST, Method::DELETE])
        .allow_any_origin();

    let cc = config.clone();
    let get_settings = warp::get()
        .and(warp::path!("api" / "settings"))
        .map(move || warp::reply::json(&cc.lock().unwrap().get_settings()));

    let player_ws_path = warp::path!("api" / "player")
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
        .or(get_settings)
        .or(ui_static_content)
        .with(cors);
    let ws_handle = tokio::task::spawn(async move {
        loop {
            let state_change_event = state_changes_receiver.try_recv();
            if state_change_event.is_err() {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
            trace!("Received state changed event {:?}", state_change_event);
            let state_change_event = state_change_event.expect("Failed to receive command.");
            notify_users(
                &users_notify,
                state_change_event,
                last_state_change_message_2.clone(),
            )
            .await;
        }
    });
    let http_handle = warp::serve(routes).run(([0, 0, 0, 0], 8000));
    (http_handle, ws_handle)
}
mod filters {
    use api_models::settings::Settings;
    use warp::Filter;

    use super::{handlers, Config};

    pub fn settings_save(
        config: Config,
    ) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::path!("api" / "settings"))
            .and(json_body())
            .and(with_config(config))
            .and_then(handlers::save_settings)
    }
    fn with_config(
        config: Config,
    ) -> impl Filter<Extract = (Config,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || config.clone())
    }

    fn json_body() -> impl Filter<Extract = (Settings,), Error = warp::Rejection> + Clone {
        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        warp::body::json()
    }
}

mod handlers {
    use std::convert::Infallible;

    use api_models::settings::Settings;
    use warp::hyper::StatusCode;

    use super::Config;
    pub async fn save_settings(
        settings: Settings,
        config: Config,
    ) -> Result<impl warp::Reply, Infallible> {
        debug!("Settings to save {:?}", settings);
        config.lock().unwrap().save_settings(&settings);
        Ok(StatusCode::CREATED)
    }
}

async fn notify_users(
    users_to_notify: &Users,
    status_change_event: StatusChangeEvent,
    last_state_change_message_2: LastStatusMessages,
) {
    let json_msg = serde_json::to_string(&status_change_event).unwrap();
    match status_change_event {
        StatusChangeEvent::CurrentTrackInfoChanged(_) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.current_track_info = Some(json_msg.clone());
            }
        }
        StatusChangeEvent::StreamerStatusChanged(_) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.streamer_status = Some(json_msg.clone());
            }
        }
        StatusChangeEvent::PlayerInfoChanged(_) => {
            let last = last_state_change_message_2.try_write();
            if last.is_ok() {
                let mut ls = last.unwrap();
                ls.player_info = Some(json_msg.clone());
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
    input_commands_tx: SyncSender<Command>,
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

    let last = last_state_message.try_read();
    if let Ok(last) = last {
        if last.current_track_info.is_some() {
            tx.send(Ok(Message::text(
                last.current_track_info.as_ref().unwrap().clone(),
            )))
            .unwrap();
        }
        if last.player_info.is_some() {
            tx.send(Ok(Message::text(
                last.player_info.as_ref().unwrap().clone(),
            )))
            .unwrap();
        }
        if last.streamer_status.is_some() {
            tx.send(Ok(Message::text(
                last.streamer_status.as_ref().unwrap().clone(),
            )))
            .unwrap();
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
            input_commands_tx.send(cmd).unwrap();
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
