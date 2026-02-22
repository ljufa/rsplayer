use std::io;

use anyhow::Result;
use api_models::common::{PlayerCommand, SystemCommand, UserCommand};
use log::debug;
use tokio::net::UnixStream;
use tokio::sync::mpsc::Sender;

pub struct IrService {
    stream: UnixStream,
    user_command_tx: Sender<UserCommand>,
    system_commands_tx: Sender<SystemCommand>,
}

impl IrService {
    pub async fn new(user_command_tx: Sender<UserCommand>, system_commands_tx: Sender<SystemCommand>) -> Result<Self> {
        let stream = UnixStream::connect("/var/run/lirc/lircd").await?;
        Ok(Self {
            stream,
            user_command_tx,
            system_commands_tx,
        })
    }

    pub async fn run(&mut self) {
        let mut buf = [0; 1024];
        loop {
            _ = self.stream.readable().await;
            match self.stream.try_read(&mut buf) {
                Ok(n) => {
                    if n == 0 {
                        continue;
                    }
                    let s = String::from_utf8_lossy(&buf[..n]);
                    for line in s.lines() {
                        self.dispatch_command(line.to_string()).await;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    log::error!("Failed to read IR socket. Will stop thread: {e}");
                    break;
                }
            }
        }
    }

    async fn dispatch_command(&self, cmd: String) {
        debug!("Lirc command: {cmd:?}");
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.len() < 4 {
            return;
        }
        let key_code = parts[2];
        let remote_name = parts[3];
        debug!("keycode:{key_code}; remotename:{remote_name}");
        if remote_name == "Apple_A1156" {
            match key_code {
                "KEY_KPPLUS" => self.system_commands_tx.send(SystemCommand::VolUp).await.unwrap(),
                "KEY_KPMINUS" => self.system_commands_tx.send(SystemCommand::VolDown).await.unwrap(),
                "KEY_FASTFORWARD" => self
                    .user_command_tx
                    .send(UserCommand::Player(PlayerCommand::Next))
                    .await
                    .unwrap(),
                "KEY_REWIND" => self
                    .user_command_tx
                    .send(UserCommand::Player(PlayerCommand::Prev))
                    .await
                    .unwrap(),
                "KEY_PLAY" => self
                    .user_command_tx
                    .send(UserCommand::Player(PlayerCommand::TogglePlay))
                    .await
                    .unwrap(),
                _ => {}
            }
        }
    }
}
