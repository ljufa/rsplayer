use std::str::FromStr;

use api_models::{
    common::{PlayerCommand, SystemCommand, UserCommand, Volume},
    state::StateChangeEvent,
};
use log::{debug, error, info};
use tokio::sync::mpsc::Sender;

pub async fn receive_commands(
    player_commands_tx: Sender<UserCommand>,
    system_commands_tx: Sender<SystemCommand>,
    state_changes_tx: tokio::sync::broadcast::Sender<StateChangeEvent>,
    port: Box<dyn serialport::SerialPort>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);

    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(port);
        let mut line_buffer = String::new();
        info!("UART reading thread started.");
        loop {
            info!("Reading line from UART...");
            match reader.read_line(&mut line_buffer) {
                Ok(0) => { // EOF
                    info!("UART stream ended.");
                    break;
                }
                Ok(bytes_read) => {
                    info!("Read {} bytes from UART: {}", bytes_read, line_buffer.trim());
                    if tx.blocking_send(line_buffer.clone()).is_err() {
                        error!("Failed to send received UART message to processing task.");
                        break;
                    }
                    line_buffer.clear();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    info!("UART read timed out.");
                    continue;
                }
                Err(e) => {
                    error!("Error reading from UART: {}", e);
                    break;
                }
            }
        }
    });

    while let Some(msg) = rx.recv().await {
        let msg = msg.trim();
        if msg.is_empty() {
            continue;
        }
        info!("Uart Received: {}", msg);
        if msg == "PowerOff" {
            system_commands_tx.send(SystemCommand::PowerOff).await.expect("");
        } else {
            if msg.starts_with("CurVolume="){
                if let Some((_, vol_str)) = msg.split_once('=') {
                    if let Ok(vol) = vol_str.parse::<u8>() {
                        info!("Parsed volume: {}", vol);
                        let volume = Volume { current: vol, ..Volume::default() };
                        _ = state_changes_tx.send(StateChangeEvent::VolumeChangeEvent(volume)).unwrap();
                    }
                }
            }
            if let Ok(pc) = PlayerCommand::from_str(msg) {
                debug!("Parsed command: {:?}", pc);
                player_commands_tx
                    .send(UserCommand::Player(pc))
                    .await
                    .expect("Unable to send command");
            }
        }
    }
}

pub fn get_all_serial_devices() -> Vec<String> {
    let mut devices = vec![];
    for entry in std::fs::read_dir("/dev").unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.iter().any(|s| s.to_str().unwrap().starts_with("ttyA")) {
            devices.push(path.to_str().unwrap().to_string());
        }
    }
    devices
}