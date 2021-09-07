use futures::FutureExt;
use futures::StreamExt;
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

use crate::{
    common::{Command, CommandEvent, DPLAY_CONFIG_DIR_PATH},
    config::Configuration,
};

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Result<Message, warp::Error>>>>>;
type LastStatusMessages = Arc<RwLock<LastStatus>>;

struct LastStatus {
    player_status: Option<String>,
    dac_status: Option<String>,
    streamer_status: Option<String>,
}
impl Default for LastStatus {
    fn default() -> Self {
        LastStatus {
            player_status: None,
            dac_status: None,
            streamer_status: None,
        }
    }
}

pub async fn start(
    mut state_changes_receiver: Receiver<CommandEvent>,
    input_commands_tx: SyncSender<Command>,
    config: Arc<Mutex<Configuration>>,
) {
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

    let settings = warp::get()
        .and(warp::path!("api" / "settings"))
        .map(move || warp::reply::json(&config.lock().expect("").get_settings()));

    let player = warp::path!("api" / "player")
        .and(warp::ws())
        .and(users_f)
        .and(input_commands_tx)
        .and(last_state_change_message_f)
        .map(
            |ws: warp::ws::Ws, users, input_commands, last_state_message| {
                // And then our closure will be called when it completes...
                ws.on_upgrade(|websocket| {
                    user_connected(websocket, users, input_commands, last_state_message)
                })
            },
        );
    let ui = warp::get().and(warp::fs::dir(format!("{}ui", DPLAY_CONFIG_DIR_PATH)));
    let routes = player.or(settings).or(ui).with(cors);
    tokio::task::spawn(async move {
        loop {
            let cmd = state_changes_receiver.try_recv();
            if cmd.is_err() {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
            debug!("Received event {:?}", cmd);
            let cmd = cmd.expect("Failed to receive command.");
            match cmd {
                CommandEvent::PlayerStatusChanged(dsc) => {
                    let json_cmd = serde_json::to_string(&dsc).unwrap();
                    debug!("Cmd json {}", json_cmd);
                    let last = last_state_change_message_2.try_write();
                    if last.is_ok() {
                        let mut ls = last.unwrap();
                        ls.player_status = Some(json_cmd.clone());
                        // debug!("Last msg {}", ls);
                    }
                    notify_users(&users_notify, json_cmd).await;
                }
                CommandEvent::StreamerStatusChanged(sstat) => {
                    let json_cmd = serde_json::to_string(&sstat).unwrap();
                    debug!("Cmd json {}", json_cmd);
                    let last = last_state_change_message_2.try_write();
                    if last.is_ok() {
                        let mut ls = last.unwrap();
                        ls.streamer_status = Some(json_cmd.clone());
                        // debug!("Last msg {}", ls);
                    }
                    notify_users(&users_notify, json_cmd).await;
                }
                CommandEvent::DacStatusChanged(dacs) => {
                    let json_cmd = serde_json::to_string(&dacs).unwrap();
                    debug!("Cmd json {}", json_cmd);
                    let last = last_state_change_message_2.try_write();
                    if last.is_ok() {
                        let mut ls = last.unwrap();
                        ls.dac_status = Some(json_cmd.clone());
                        // debug!("Last msg {}", ls);
                    }
                    notify_users(&users_notify, json_cmd).await;
                }
                _ => {}
            }
        }
    });
    warp::serve(routes).run(([0, 0, 0, 0], 8000)).await
}

async fn notify_users(
    users_notify: &Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Result<Message, warp::Error>>>>>,
    json_cmd: String,
) {
    for (&uid, tx) in users_notify.read().await.iter() {
        if let Err(_disconnected) = tx.send(Ok(Message::text(json_cmd.clone()))) {
            // The tx is disconnected, our `user_disconnected` code
            // should be happening in another task, nothing more to
            // do here.
        }
    }
}
// fn notify_users()

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
        if last.player_status.is_some() {
            // debug!("Send last event to new user {}", last);
            tx.send(Ok(Message::text(
                last.player_status.as_ref().unwrap().clone(),
            )))
            .unwrap();
        }
        if last.dac_status.is_some() {
            // debug!("Send last event to new user {}", last);
            tx.send(Ok(Message::text(last.dac_status.as_ref().unwrap().clone())))
                .unwrap();
        }
        if last.streamer_status.is_some() {
            // debug!("Send last event to new user {}", last);
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
    let users2 = users.clone();
    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(my_id, &users2).await;
}

async fn user_disconnected(my_id: usize, users: &Users) {
    debug!("good bye user: {}", my_id);

    // Stream closed up, so remove from the user list
    users.write().await.remove(&my_id);
}
