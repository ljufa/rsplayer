use anyhow::Result;
use api_models::{
    common::{PlayerCommand, SystemCommand, UserCommand, Volume},
    state::StateChangeEvent,
};
use log::{debug, error, info, trace};
use serialport::SerialPort;
use std::result::Result::Ok;
use std::str::FromStr;
use std::time::Duration;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
};
use tokio::sync::{
    broadcast::error::{RecvError, TryRecvError},
    mpsc::Sender,
};

pub struct UsbService {
    port: Mutex<Option<Box<dyn SerialPort>>>,
    baud_rate: u32,
    last_song_cache: Mutex<Option<(String, String, String)>>,
    last_playback_mode_cache: Mutex<Option<String>>,
}

impl UsbService {
    pub fn new(baud_rate: u32) -> Self {
        Self {
            port: Mutex::new(None),
            baud_rate,
            last_song_cache: Mutex::new(None),
            last_playback_mode_cache: Mutex::new(None),
        }
    }

    pub fn send_command(&self, command: &str) -> Result<()> {
        let message = format!("{command}\n");

        let mut port_guard = self.port.lock().unwrap();
        if let Some(port) = port_guard.as_mut() {
            port.write_all(message.as_bytes()).and_then(|()| port.flush())?;
            trace!("Written command: {command}");
            Ok(())
        } else {
            Err(anyhow::anyhow!("USB port not connected"))
        }
    }

    pub fn try_reconnect(&self) -> Result<()> {
        get_rsplayer_firmware_usb_link().map_or_else(
            || {
                debug!("No USB device found for reconnection.");
                Err(anyhow::anyhow!("Device not found"))
            },
            |new_path| match serialport::new(&new_path, self.baud_rate)
                .timeout(Duration::from_secs(1))
                .open()
            {
                Ok(new_port) => {
                    info!("Reconnected to USB device at {new_path}");
                    {
                        let mut port_guard = self.port.lock().unwrap();
                        *port_guard = Some(new_port);
                    }
                    let cached_song = self.last_song_cache.lock().unwrap().clone();
                    if let Some((t, a, al)) = cached_song {
                        debug!("Resending cached track info: {t} - {a}");
                        let _ = self.send_command(&format!("SetTrack({t}|{a}|{al})"));
                    }
                    let cached_mode = self.last_playback_mode_cache.lock().unwrap().clone();
                    if let Some(mode) = cached_mode {
                        debug!("Resending cached playback mode: {mode}");
                        let _ = self.send_command(&format!("SetPlaybackMode({mode})"));
                    }
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to open port at {new_path}: {e}");
                    Err(anyhow::anyhow!("Failed to open port"))
                }
            },
        )
    }

    pub fn send_track_info(&self, title: &str, artist: &str, album: &str) -> Result<()> {
        *self.last_song_cache.lock().unwrap() = Some((title.to_string(), artist.to_string(), album.to_string()));
        self.send_command(&format!("SetTrack({title}|{artist}|{album})"))
    }

    pub fn send_progress(&self, current: &str, total: &str, percent: u8) -> Result<()> {
        self.send_command(&format!("SetProgress({current}|{total}|{percent})"))
    }

    pub fn send_vu_level(&self, left: u8, right: u8) -> Result<()> {
        self.send_command(&format!("SetVU({left}|{right})"))
    }

    pub fn send_power_command(&self, on: bool) -> Result<()> {
        let cmd = if on { "PowerOn" } else { "PowerOff" };
        self.send_command(cmd)
    }
}

pub fn get_rsplayer_firmware_usb_link() -> Option<String> {
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            if let serialport::SerialPortType::UsbPort(info) = p.port_type {
                debug!(
                    "Checking USB port: {:?} (Product: {:?}, VID: {:?}, PID: {:?})",
                    p.port_name, info.product, info.vid, info.pid
                );
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
                let mut port_guard = service.port.lock().unwrap();
                port_guard.as_mut().map_or_else(
                    || {
                        Err(serialport::Error::new(
                            serialport::ErrorKind::NoDevice,
                            "No port available",
                        ))
                    },
                    |p| {
                        debug!("Port available, attempting to clone...");
                        p.try_clone()
                    },
                )
            };

            match port_result {
                Ok(port) => {
                    let mut reader = BufReader::new(port);
                    let mut line_buffer = String::new();
                    info!("USB Listener loop started successfully");

                    loop {
                        match reader.read_line(&mut line_buffer) {
                            Ok(0) => {
                                // EOF
                                error!("USB EOF, connection lost.");
                                break;
                            }
                            Ok(_) => {
                                let msg = line_buffer.trim();
                                if msg.is_empty() {
                                    continue;
                                }
                                debug!("Got usb message: {msg}");
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
                                    if msg.starts_with("PowerState=") {
                                        if let Some((_, state_str)) = msg.split_once('=') {
                                            let is_on = state_str == "1";
                                            info!("Firmware power state changed: {is_on}");
                                            _ = state_changes_tx.send(StateChangeEvent::RSPlayerFirmwarePowerEvent(is_on));
                                        }
                                    }
                                    if msg == "CyclePlaybackMode" {
                                        _ = player_commands_tx
                                            .blocking_send(UserCommand::Player(PlayerCommand::CyclePlaybackMode));
                                    } else if msg == "SeekForward" {
                                        _ = player_commands_tx
                                            .blocking_send(UserCommand::Player(PlayerCommand::SeekForward));
                                    } else if msg == "SeekBackward" {
                                        _ = player_commands_tx
                                            .blocking_send(UserCommand::Player(PlayerCommand::SeekBackward));
                                    } else if let Ok(pc) = PlayerCommand::from_str(msg) {
                                        _ = player_commands_tx.blocking_send(UserCommand::Player(pc));
                                    }
                                }
                                line_buffer.clear();
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                            Err(e) => {
                                error!("Error reading from USB: {e}");
                                break;
                            }
                        }
                    }
                    // If we broke out of the inner loop, it means the connection is likely dead.
                    // Clear the port so try_reconnect can start fresh.
                    info!("USB connection broken, clearing port handle.");
                    let mut port_guard = service.port.lock().unwrap();
                    *port_guard = None;
                }
                Err(e) => {
                    debug!("Failed to get port handle: {e}");
                }
            }

            // Connection lost or failed to start, try to reconnect
            debug!("Waiting 2 seconds before next reconnection attempt...");
            sleep(Duration::from_secs(2));

            if let Err(e) = service.try_reconnect() {
                debug!("Reconnect attempt failed: {e}");
            }
        }
    });
}

pub fn start_state_sync(
    service: Arc<UsbService>,
    state_changes_tx: &tokio::sync::broadcast::Sender<StateChangeEvent>,
) {
    let mut rx = state_changes_tx.subscribe();

    tokio::spawn(async move {
        loop {
            // Blocking wait for the first event
            let first_event = match rx.recv().await {
                Ok(e) => e,
                Err(RecvError::Lagged(count)) => {
                    error!("USB state sync channel lagged by {count} messages");
                    continue;
                }
                Err(RecvError::Closed) => {
                    error!("USB state sync channel closed");
                    break;
                }
            };

            let mut events = vec![first_event];

            // Drain any other pending events
            loop {
                match rx.try_recv() {
                    Ok(e) => events.push(e),
                    Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                    Err(TryRecvError::Lagged(count)) => {
                        error!("USB state sync channel lagged by {count} messages during drain");
                        continue;
                    }
                }
                if events.len() > 100 {
                    break;
                }
            }

            let mut last_vu: Option<(u8, u8)> = None;

            for event in events {
                match event {
                    StateChangeEvent::VUEvent(l, r) => {
                        last_vu = Some((l, r));
                    }
                    _ => {
                        process_event(&service, event);
                    }
                }
            }

            if let Some((l, r)) = last_vu {
                let _ = service.send_vu_level(l, r);
            }
        }
    });
}

fn process_event(service: &UsbService, event: StateChangeEvent) {
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
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let percent = if progress.total_time.as_secs() > 0 {
                ((progress.current_time.as_secs_f32() / progress.total_time.as_secs_f32()) * 100.0) as u8
            } else {
                0
            };
            let _ = service.send_progress(&current, &total, percent);
        }
        StateChangeEvent::PlaybackModeChangedEvent(mode) => {
            let mode_str: &str = mode.into();
            debug!("PlaybackModeChangedEvent received: {mode_str}");
            *service.last_playback_mode_cache.lock().unwrap() = Some(mode_str.to_string());
            let _ = service.send_command(&format!("SetPlaybackMode({mode_str})"));
        }
        _ => {}
    }
}
pub type ArcUsbService = Arc<UsbService>;
