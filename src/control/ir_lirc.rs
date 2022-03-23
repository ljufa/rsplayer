use api_models::player::Command;
use cfg_if::cfg_if;
use tokio::sync::mpsc::Sender;

use crate::common;

pub async fn listen(input_comands_tx: Sender<Command>) {
    cfg_if! {
        if #[cfg(feature="hw_ir_control")] {
            hw_ir::listen(input_commands_tx)
        } else{
            common::no_op_future().await;
        }
    }
}

#[cfg(feature = "hw_ir_control")]
mod hw_ir {
    use std::io;
    use std::str;

    use api_models::player::Command;

    use failure::_core::time::Duration;

    use tokio::net::UnixStream;
    use tokio::sync::mpsc::Sender;
    const REMOTE_MAKER: &'static str = "dplayd";

    async fn listen(input_comands_tx: Sender<Command>) {
        info!("Start IR Control thread.");
        let stream = UnixStream::connect("/var/run/lirc/lircd").await.unwrap();

        loop {
            trace!("Loop cycle");
            stream.readable().await;
            let mut bytes = [0; 60];
            match stream.try_read(&mut bytes) {
                Ok(n) => {
                    debug!("Read {} bytes from socket", n);
                    let result = str::from_utf8(&bytes).unwrap();
                    let remote_maker = result.find(REMOTE_MAKER);
                    if remote_maker.is_none() || result.len() < 18 {
                        continue;
                    }
                    let end = remote_maker.unwrap();
                    if end <= 18 {
                        continue;
                    }
                    let key = &result[17..end - 1];
                    match key {
                        "00 KEY_PLAY" => {
                            input_comands_tx.send(Command::Play).await.expect("Error");
                        }
                        "00 KEY_STOP" => {
                            input_comands_tx.send(Command::Pause).await.expect("Error");
                        }
                        "00 KEY_NEXTSONG" => {
                            input_comands_tx.send(Command::Next).await.expect("Error");
                        }
                        "00 KEY_PREVIOUSSONG" => {
                            input_comands_tx.send(Command::Prev).await.expect("Error");
                        }
                        "00 KEY_EJECTCD" => {
                            input_comands_tx
                                .send(Command::ChangeAudioOutput)
                                .await
                                .expect("Error");
                            std::thread::sleep(Duration::from_secs(1));
                        }
                        "05 KEY_POWER" => {
                            input_comands_tx
                                .send(Command::PowerOff)
                                .await
                                .expect("Error");
                            std::thread::sleep(Duration::from_secs(10));
                        }
                        _ => {
                            let key_str = String::from(key);
                            if key_str.ends_with("KEY_DOWN") {
                                input_comands_tx
                                    .send(Command::VolDown)
                                    .await
                                    .expect("Error");
                            }
                            if key_str.ends_with("KEY_UP") {
                                input_comands_tx.send(Command::VolUp).await.expect("Error");
                            }
                            if key_str.ends_with("KEY_NEXT") {
                                input_comands_tx
                                    .send(Command::Rewind(5))
                                    .await
                                    .expect("Error");
                            }
                            if key_str.ends_with("KEY_PREVIOUS") {
                                input_comands_tx
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
    }
}
