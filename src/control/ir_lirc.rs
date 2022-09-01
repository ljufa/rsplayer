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
                        "00 KEY_UP" => {
                            input_commands_tx.send(Command::Play).await.expect("Error");
                        }
                        "00 KEY_DOWN" => {
                            input_commands_tx.send(Command::Pause).await.expect("Error");
                        }
                        "00 KEY_NEXT" => {
                            input_commands_tx.send(Command::Next).await.expect("Error");
                        }
                        "00 KEY_PREVIOUS" => {
                            input_commands_tx.send(Command::Prev).await.expect("Error");
                        }
                        "00 BTN_MOUSE" => {
                            input_commands_tx
                                .send(Command::RandomToggle)
                                .await
                                .expect("Error");
                        }
                        "00 KEY_MEDIA" => {
                            input_commands_tx
                                .send(Command::LoadPlaylist(
                                    "mpd_playlist_saved_remote".to_string(),
                                ))
                                .await
                                .expect("Error");
                        }
                        "00 KEY_RADIO" => {
                            input_commands_tx
                                .send(Command::LoadPlaylist(
                                    "mpd_playlist_saved_radio".to_string(),
                                ))
                                .await
                                .expect("Error");
                        }
                        "00 KEY_WWW" => {
                            input_commands_tx
                                .send(Command::LoadPlaylist(
                                    "mpd_playlist_saved_local".to_string(),
                                ))
                                .await
                                .expect("Error");
                        }
                        "00 KEY_MENU" => {
                            input_commands_tx
                                .send(Command::ChangeAudioOutput)
                                .await
                                .expect("Error");
                        }
                        "05 KEY_POWER" => {
                            input_commands_tx
                                .send(Command::PowerOff)
                                .await
                                .expect("Error");
                        }
                        _ => {
                            let key_str = String::from(key);
                            if key_str.ends_with("KEY_VOLUMEDOWN") {
                                input_commands_tx
                                    .send(Command::VolDown)
                                    .await
                                    .expect("Error");
                            }
                            if key_str.ends_with("KEY_VOLUMEUP") {
                                input_commands_tx.send(Command::VolUp).await.expect("Error");
                            }
                            if key_str.ends_with("KEY_RIGHT") {
                                input_commands_tx
                                    .send(Command::Rewind(5))
                                    .await
                                    .expect("Error");
                            }
                            if key_str.ends_with("KEY_LEFT") {
                                input_commands_tx
                                    .send(Command::Rewind(-5))
                                    .await
                                    .expect("Error");
                            }
                        }
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
