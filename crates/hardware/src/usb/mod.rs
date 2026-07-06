//! USB serial link to the `RSPlayer` front-panel firmware.
//!
//! Speaks the `wire` crate protocol (postcard + COBS frames): a sender
//! thread mirrors `StateChangeEvent`s (track, progress, VU, mode) to the
//! panel display, a receiver thread turns panel input (`FwToHost`) into
//! player/system commands. Reconnects when the device disappears.

use anyhow::Result;
use api_models::{
    common::{PlayerCommand, SystemCommand, UserCommand},
    state::StateChangeEvent,
};
use log::{debug, error, info, trace};
use serialport::SerialPort;
use std::result::Result::Ok;
use std::time::Duration;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
};
use tokio::sync::{
    broadcast::error::{RecvError, TryRecvError},
    mpsc::Sender,
};
use wire::{ALBUM_LEN, ARTIST_LEN, FwPlayerCmd, FwToHost, HostToFw, MAX_FRAME, TIME_LEN, TITLE_LEN};

pub struct UsbService {
    port: Mutex<Option<Box<dyn SerialPort>>>,
    baud_rate: u32,
    last_song_cache: Mutex<Option<(String, String, String)>>,
    last_playback_mode_cache: Mutex<Option<wire::PlaybackMode>>,
}

/// Truncate `s` so the result fits in `heapless::String<N>` without splitting
/// a multi-byte UTF-8 sequence.
fn clamp<const N: usize>(s: &str) -> wire::heapless::String<N> {
    let mut end = s.len().min(N);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = wire::heapless::String::<N>::new();
    let _ = out.push_str(&s[..end]);
    out
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

    /// Encode `msg` with postcard + COBS framing and write it to the port.
    pub fn send(&self, msg: &HostToFw) -> Result<()> {
        let mut buf = [0u8; MAX_FRAME];
        let frame = postcard::to_slice_cobs(msg, &mut buf).map_err(|e| anyhow::anyhow!("postcard encode failed: {e}"))?;

        let mut port_guard = self.port.lock().expect("lock poisoned");
        if let Some(port) = port_guard.as_mut() {
            port.write_all(frame).and_then(|()| port.flush())?;
            trace!("Sent {msg:?} ({} bytes)", frame.len());
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
            |new_path| match serialport::new(&new_path, self.baud_rate).timeout(Duration::from_secs(1)).open() {
                Ok(new_port) => {
                    info!("Reconnected to USB device at {new_path}");
                    {
                        let mut port_guard = self.port.lock().expect("lock poisoned");
                        *port_guard = Some(new_port);
                    }
                    self.resync_state();
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
        *self.last_song_cache.lock().expect("lock poisoned") = Some((title.to_string(), artist.to_string(), album.to_string()));
        self.send(&HostToFw::Track {
            title: clamp::<TITLE_LEN>(title),
            artist: clamp::<ARTIST_LEN>(artist),
            album: clamp::<ALBUM_LEN>(album),
        })
    }

    pub fn send_progress(&self, current: &str, total: &str, percent: u8) -> Result<()> {
        self.send(&HostToFw::Progress {
            current: clamp::<TIME_LEN>(current),
            total: clamp::<TIME_LEN>(total),
            percent,
        })
    }

    pub fn send_vu_level(&self, left: u8, right: u8) -> Result<()> {
        self.send(&HostToFw::Vu { left, right })
    }

    pub fn send_power_command(&self, on: bool) -> Result<()> {
        self.send(if on { &HostToFw::PowerOn } else { &HostToFw::PowerOff })
    }

    /// Pushes everything the panel needs after a (re)connect or firmware
    /// power-on: cached track info and playback mode, plus a volume query so
    /// the host learns the firmware's current level. Idempotent.
    pub fn resync_state(&self) {
        let cached_song = self.last_song_cache.lock().expect("lock poisoned").clone();
        if let Some((t, a, al)) = cached_song {
            debug!("Resending cached track info: {t} - {a}");
            let _ = self.send_track_info(&t, &a, &al);
        }
        let cached_mode = *self.last_playback_mode_cache.lock().expect("lock poisoned");
        if let Some(mode) = cached_mode {
            debug!("Resending cached playback mode: {mode:?}");
            let _ = self.send(&HostToFw::PlaybackMode(mode));
        }
        let _ = self.send(&HostToFw::QueryVolume);
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

pub fn spawn_receiver_thread(
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
                let mut port_guard = service.port.lock().expect("lock poisoned");
                port_guard.as_mut().map_or_else(
                    || Err(serialport::Error::new(serialport::ErrorKind::NoDevice, "No port available")),
                    |p| {
                        debug!("Port available, attempting to clone...");
                        p.try_clone()
                    },
                )
            };

            match port_result {
                Ok(port) => {
                    let mut reader = BufReader::new(port);
                    let mut frame: Vec<u8> = Vec::with_capacity(MAX_FRAME);
                    info!("USB Listener loop started successfully");

                    loop {
                        match reader.read_until(0x00, &mut frame) {
                            Ok(0) => {
                                error!("USB EOF, connection lost.");
                                break;
                            }
                            Ok(_) => {
                                // read_until only returns without the delimiter
                                // at EOF; loop again so Ok(0) reports it.
                                if frame.last() != Some(&0x00) {
                                    continue;
                                }
                                // Strip trailing COBS sentinel; postcard decodes the body in place.
                                frame.pop();
                                if !frame.is_empty() {
                                    match postcard::from_bytes_cobs::<FwToHost>(&mut frame) {
                                        Ok(msg) => {
                                            debug!("Got fw message: {msg:?}");
                                            dispatch_fw_to_host_msg(&service, msg, &player_commands_tx, &system_commands_tx, &state_changes_tx);
                                        }
                                        Err(e) => {
                                            error!("Failed to decode fw message ({} bytes): {e}", frame.len());
                                        }
                                    }
                                }
                                frame.clear();
                            }
                            // Timeout between panel messages: keep any partial
                            // frame — the rest of it arrives with the next read.
                            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                            Err(e) => {
                                error!("Error reading from USB: {e}");
                                break;
                            }
                        }
                    }
                    info!("USB connection broken, clearing port handle.");
                    let mut port_guard = service.port.lock().expect("lock poisoned");
                    *port_guard = None;
                }
                Err(e) => {
                    debug!("Failed to get port handle: {e}");
                }
            }

            debug!("Waiting 2 seconds before next reconnection attempt...");
            sleep(Duration::from_secs(2));

            if let Err(e) = service.try_reconnect() {
                debug!("Reconnect attempt failed: {e}");
            }
        }
    });
}

fn dispatch_fw_to_host_msg(
    service: &UsbService,
    msg: FwToHost,
    player_commands_tx: &Sender<UserCommand>,
    system_commands_tx: &Sender<SystemCommand>,
    state_changes_tx: &tokio::sync::broadcast::Sender<StateChangeEvent>,
) {
    match msg {
        FwToHost::Volume(vol) => {
            let _ = system_commands_tx.blocking_send(SystemCommand::ReportVolume(vol));
        }
        FwToHost::Power(is_on) => {
            info!("Firmware power state changed: {is_on}");
            let _ = state_changes_tx.send(StateChangeEvent::RSPlayerFirmwarePowerEvent(is_on));
            if is_on {
                // Panel just came up (power-on or USB reconnect): replay the
                // cached display state so it doesn't sit blank until the next
                // natural event.
                service.resync_state();
            }
        }
        FwToHost::Player(cmd) => {
            let pc = match cmd {
                FwPlayerCmd::Next => PlayerCommand::Next,
                FwPlayerCmd::Prev => PlayerCommand::Prev,
                FwPlayerCmd::TogglePlay => PlayerCommand::TogglePlay,
                FwPlayerCmd::Stop => PlayerCommand::Stop,
                FwPlayerCmd::SeekForward => PlayerCommand::SeekForward,
                FwPlayerCmd::SeekBackward => PlayerCommand::SeekBackward,
                FwPlayerCmd::CyclePlaybackMode => PlayerCommand::CyclePlaybackMode,
            };
            let _ = player_commands_tx.blocking_send(UserCommand::Player(pc));
        }
    }
}

pub fn spawn_sender_thread(service: Arc<UsbService>, state_changes_tx: &tokio::sync::broadcast::Sender<StateChangeEvent>) {
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
                        dispatch_host_to_fw_msg(&service, event);
                    }
                }
            }

            if let Some((l, r)) = last_vu {
                let _ = service.send_vu_level(l, r);
            }
        }
    });
}

fn dispatch_host_to_fw_msg(service: &UsbService, event: StateChangeEvent) {
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
            debug!("PlaybackModeChangedEvent received: {mode:?}");
            *service.last_playback_mode_cache.lock().expect("lock poisoned") = Some(mode);
            let _ = service.send(&HostToFw::PlaybackMode(mode));
        }
        _ => {}
    }
}
pub type ArcUsbService = Arc<UsbService>;
