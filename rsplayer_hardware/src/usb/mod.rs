use anyhow::Result;
use api_models::{
    common::{PlayerCommand, SystemCommand, UserCommand, Volume},
    state::StateChangeEvent,
};
use log::{debug, error, info};
use serialport::SerialPort;
use std::result::Result::Ok;
use std::str::FromStr;
use std::time::Duration;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
};
use tokio::sync::mpsc::Sender;

pub struct UsbService {
    port: Mutex<Box<dyn SerialPort>>,
    baud_rate: u32,
}

impl UsbService {
    pub fn new(path: &str, baud_rate: u32) -> Result<Self> {
        let port = serialport::new(path, baud_rate)
            .timeout(Duration::from_secs(1))
            .open()
            .expect("Failed to open port");
        Ok(Self {
            port: Mutex::new(port),
            baud_rate,
        })
    }

    pub fn send_command(&self, command: &str) -> Result<()> {
        let message = format!("{command}\n");

        let result = {
            let mut port = self.port.lock().unwrap();
            port.write_all(message.as_bytes()).and_then(|()| port.flush())
        };

        if result.is_ok() {
            debug!("Written command: {command}");
            return Ok(());
        }

        error!("Write failed, attempting recovery...");
        self.try_reconnect()?;

        let mut port = self.port.lock().unwrap();
        port.write_all(message.as_bytes())?;
        port.flush()?;
        debug!("Written command after reconnect: {command}");
        Ok(())
    }

    pub fn try_reconnect(&self) -> Result<()> {
        if let Some(new_path) = get_rsplayer_firmware_usb_link() {
            match serialport::new(&new_path, self.baud_rate)
                .timeout(Duration::from_secs(1))
                .open()
            {
                Ok(new_port) => {
                    info!("Reconnected to USB device at {new_path}");
                    let mut port_guard = self.port.lock().unwrap();
                    *port_guard = new_port;
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to open port at {new_path}: {e}");
                    Err(anyhow::anyhow!("Failed to open port"))
                }
            }
        } else {
            debug!("No USB device found for reconnection.");
            Err(anyhow::anyhow!("Device not found"))
        }
    }

    pub fn send_track_info(&self, title: &str, artist: &str, album: &str) -> Result<()> {
        self.send_command(&format!("SetTrack({title}|{artist}|{album})"))
    }

    pub fn send_progress(&self, current: &str, total: &str, percent: u8) -> Result<()> {
        self.send_command(&format!("SetProgress({current}|{total}|{percent})"))
    }
}

pub fn get_rsplayer_firmware_usb_link() -> Option<String> {
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            if let serialport::SerialPortType::UsbPort(info) = p.port_type {
                if info.product == Some("rsplayer-firmware-v1.0".to_owned()) {
                    return Some(p.port_name);
                }
            }
        }
    }
    None
}

pub fn start_listening(
    service: Arc<UsbService>,
    player_commands_tx: Sender<UserCommand>,
    system_commands_tx: Sender<SystemCommand>,
    state_changes_tx: tokio::sync::broadcast::Sender<StateChangeEvent>,
) {
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader};

        loop {
            // Try to acquire a working port reader
            let port_result = {
                let port_guard = service.port.lock().unwrap();
                port_guard.try_clone()
            };

            match port_result {
                Ok(port) => {
                    let mut reader = BufReader::new(port);
                    let mut line_buffer = String::new();
                    info!("USB Listener loop started");

                    loop {
                        match reader.read_line(&mut line_buffer) {
                            Ok(0) => {
                                // EOF
                                error!("USB EOF, connection lost.");
                                break;
                            }
                            Ok(_) => {
                                let msg = line_buffer.trim();
                                debug!("Got usb message: {msg}");
                                if msg.is_empty() {
                                    continue;
                                }
                                if msg == "PowerOff" {
                                    _ = system_commands_tx.blocking_send(SystemCommand::PowerOff);
                                } else {
                                    if msg.starts_with("CurVolume=") {
                                        if let Some((_, vol_str)) = msg.split_once('=') {
                                            if let Ok(vol) = vol_str.parse::<u8>() {
                                                let volume = Volume {
                                                    current: vol,
                                                    ..Volume::default()
                                                };
                                                _ = state_changes_tx.send(StateChangeEvent::VolumeChangeEvent(volume));
                                            }
                                        }
                                    }
                                    if let Ok(pc) = PlayerCommand::from_str(msg) {
                                        _ = player_commands_tx.blocking_send(UserCommand::Player(pc));
                                    }
                                }
                                line_buffer.clear();
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                                debug!("Timeout usb read");
                                continue;
                            }
                            Err(e) => {
                                error!("Error reading from USB: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to clone port: {e}");
                }
            }

            // Connection lost or failed to start, try to reconnect
            error!("USB connection lost or failed. Attempting to reconnect in 2 seconds...");
            sleep(Duration::from_secs(2));

            if let Err(e) = service.try_reconnect() {
                debug!("Reconnect failed: {e}");
            }
        }
    });
}

pub fn start_state_sync(service: Arc<UsbService>, state_changes_tx: tokio::sync::broadcast::Sender<StateChangeEvent>) {
    let mut rx = state_changes_tx.subscribe();

    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                StateChangeEvent::CurrentSongEvent(song) => {
                    let _ = service.send_track_info(
                        song.title.as_deref().unwrap_or(""),
                        song.artist.as_deref().unwrap_or(""),
                        song.album.as_deref().unwrap_or(""),
                    );
                }
                StateChangeEvent::SongTimeEvent(progress) => {
                    let current = api_models::common::dur_to_string(&progress.current_time);
                    let total = api_models::common::dur_to_string(&progress.total_time);
                    let percent = if progress.total_time.as_secs() > 0 {
                        ((progress.current_time.as_secs_f32() / progress.total_time.as_secs_f32()) * 100.0) as u8
                    } else {
                        0
                    };
                    let _ = service.send_progress(&current, &total, percent);
                }
                _ => {}
            }
        }
    });
}
pub type ArcUsbService = Arc<UsbService>;
