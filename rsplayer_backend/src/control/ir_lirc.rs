use std::io;
use std::str;

use api_models::common::Command;

use tokio::net::UnixStream;
use tokio::sync::mpsc::Sender;

use crate::common::MutArcConfiguration;

pub async fn listen(input_commands_tx: Sender<Command>, config: MutArcConfiguration) {
    let ir_settings = config.lock().unwrap().get_settings().ir_control_settings;
    let maker = &ir_settings.remote_maker;

    if let Ok(stream) = UnixStream::connect(&ir_settings.input_socket_path).await {
        info!("Start IR Control thread.");
        loop {
            trace!("Loop cycle");
            _ = stream.readable().await;
            let mut bytes = [0; 60];
            match stream.try_read(&mut bytes) {
                Ok(n) => {
                    debug!("Read {} bytes from socket", n);
                    let result = str::from_utf8(&bytes).unwrap();
                    debug!("Remote maker is {:?}", result);
                    let remote_maker = result.find(maker);
                    if remote_maker.is_none() || result.len() < 18 {
                        continue;
                    }
                    let end = remote_maker.unwrap();
                    if end <= 18 {
                        continue;
                    }
                    let key = &result[17..end - 1];
                    debug!("Key is {}", key);
                    match key {
                        "00 KEY_KPMINUS" => {
                            input_commands_tx
                                .send(Command::VolDown)
                                .await
                                .expect("Error");
                        }
                        "00 KEY_KPPLUS" => {
                            input_commands_tx.send(Command::VolUp).await.expect("Error");
                        }
                        "00 KEY_FASTFORWARD" => {
                            input_commands_tx.send(Command::Next).await.expect("Error");
                        }
                        "00 KEY_REWIND" => {
                            input_commands_tx.send(Command::Prev).await.expect("Error");
                        }
                        "00 KEY_PLAY" => {
                            input_commands_tx.send(Command::Play).await.expect("Error");
                        }
                        "06 KEY_PLAY" => {
                            input_commands_tx.send(Command::Pause).await.expect("Error");
                        }
                        "06 KEY_MENU" => {
                            input_commands_tx
                                .send(Command::PowerOff)
                                .await
                                .expect("Error");
                        }
                        _ => {}
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    error!("Failed to read IR socket. Will stop thread: {}", e);
                    break;
                }
            }
        }
    } else {
        error!("Failed to open provided lirc socket");
        crate::common::no_op_future().await;
    }
}
