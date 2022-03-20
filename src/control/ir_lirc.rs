use std::io;
use std::str;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use api_models::player::Command;

use failure::_core::time::Duration;
use tokio::task::JoinHandle;

type ReadSocket = Arc<Mutex<dyn io::Read + Send>>;

const REMOTE_MAKER: &'static str = "dplayd";

pub fn start(tx: SyncSender<Command>, lirc_socket: ReadSocket) -> JoinHandle<()> {
    tokio::task::spawn(async move {
        loop {
            let mut bytes = [0; 60];
            lirc_socket
                .lock()
                .unwrap()
                .read(&mut bytes)
                .expect("Failed to read lirc socket.");
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
                    tx.send(Command::Play).expect("Error");
                }
                "00 KEY_STOP" => {
                    tx.send(Command::Pause).expect("Error");
                }
                "00 KEY_NEXTSONG" => {
                    tx.send(Command::Next).expect("Error");
                }
                "00 KEY_PREVIOUSSONG" => {
                    tx.send(Command::Prev).expect("Error");
                }
                "00 KEY_EJECTCD" => {
                    tx.send(Command::ChangeAudioOutput).expect("Error");
                    std::thread::sleep(Duration::from_secs(1));
                }
                "05 KEY_POWER" => {
                    tx.send(Command::PowerOff).expect("Error");
                    std::thread::sleep(Duration::from_secs(10));
                }

                _ => {
                    let key_str = String::from(key);
                    if key_str.ends_with("KEY_DOWN") {
                        tx.send(Command::VolDown).expect("Error");
                    }
                    if key_str.ends_with("KEY_UP") {
                        tx.send(Command::VolUp).expect("Error");
                    }
                    if key_str.ends_with("KEY_NEXT") {
                        tx.send(Command::Rewind(5)).expect("Error");
                    }
                    if key_str.ends_with("KEY_PREVIOUS") {
                        tx.send(Command::Rewind(-5)).expect("Error");
                    }
                }
            }
        }
    })
}
