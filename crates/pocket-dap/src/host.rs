use api_models::common::UserCommand;
use api_models::state::StateChangeEvent;
use futures_util::{SinkExt, StreamExt};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub enum HostEvent {
    Connected,
    Disconnected,
    State(Box<StateChangeEvent>),
}

pub fn spawn(url: String) -> (UnboundedSender<UserCommand>, Receiver<HostEvent>) {
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    thread::Builder::new()
        .name("pocket-dap-ws".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                loop {
                    match connect_async(url.clone()).await {
                        Ok((ws, _)) => {
                            let _ = ev_tx.send(HostEvent::Connected);
                            let (mut sink, mut stream) = ws.split();
                            let mut dead = false;
                            while !dead {
                                tokio::select! {
                                    cmd = cmd_rx.recv() => {
                                        let Some(cmd) = cmd else { return };
                                        if let Ok(json) = serde_json::to_string(&cmd)
                                            && sink.send(Message::Text(json.into())).await.is_err()
                                        {
                                            dead = true;
                                        }
                                    }
                                    msg = stream.next() => {
                                        match msg {
                                            Some(Ok(Message::Text(text))) => {
                                                if let Ok(ev) = serde_json::from_str::<StateChangeEvent>(text.as_str()) {
                                                    let _ = ev_tx.send(HostEvent::State(Box::new(ev)));
                                                }
                                            }
                                            Some(Ok(Message::Ping(payload))) => {
                                                if sink.send(Message::Pong(payload)).await.is_err() {
                                                    dead = true;
                                                }
                                            }
                                            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => dead = true,
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            let _ = ev_tx.send(HostEvent::Disconnected);
                        }
                        Err(_) => {
                            let _ = ev_tx.send(HostEvent::Disconnected);
                            tokio::time::sleep(Duration::from_secs(2)).await;
                        }
                    }
                }
            });
        })
        .expect("ws thread");
    (cmd_tx, ev_rx)
}
