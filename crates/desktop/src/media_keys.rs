//! OS media-key integration via souvlaki (MPRIS2 on Linux, MediaRemote on
//! macOS, SMTC on Windows). Not built on Android — there the Kotlin
//! `PlaybackService` owns the media session and talks to the backend over
//! its WebSocket.

use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;

use api_models::common::{PlayerCommand, SystemRequest, UserCommand};
use log::{info, warn};
use souvlaki::{MediaControlEvent, MediaControls, PlatformConfig};
use tokio::sync::{mpsc, oneshot};

/// The backend's command sender — `None` until the backend is up.
pub type CommandSlot = Arc<Mutex<Option<mpsc::Sender<UserCommand>>>>;

/// Start the media-key listener. `cmd_rx` yields the backend's command
/// sender once the backend is up; until then media events are dropped.
/// Returns the slot so other controls (the tray menu) can send commands too.
pub fn start(cmd_rx: oneshot::Receiver<mpsc::Sender<UserCommand>>) -> CommandSlot {
    // Shared slot — starts None, filled once the backend hands us the sender.
    let media_sender: CommandSlot = Arc::new(Mutex::new(None));
    let commands = Arc::clone(&media_sender);

    let media_sender_init = Arc::clone(&media_sender);
    tokio::spawn(async move {
        match cmd_rx.await {
            Ok(sender) => {
                if let Ok(mut guard) = media_sender_init.lock() {
                    *guard = Some(sender);
                }
            }
            Err(_) => {
                warn!("Backend did not send command sender (degraded mode?).");
            }
        }
    });

    // Dedicated thread that holds MediaControls alive for the app lifetime so
    // the OS keeps routing media key events here.
    spawn(move || {
        let config = PlatformConfig {
            // Registers org.mpris.MediaPlayer2.rsplayer on D-Bus — must stay
            // in sync with the --own-name grant in the PKGS/flatpak manifest
            // and the mpris slot `name:` in snap/snapcraft.yaml.
            dbus_name: "rsplayer",
            display_name: "RSPlayer",
            hwnd: None,
        };

        let mut controls = match MediaControls::new(config) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create media controls: {:?}", e);
                return;
            }
        };

        if let Err(e) = controls.attach(move |event: MediaControlEvent| {
            let cmd = match event {
                MediaControlEvent::Play | MediaControlEvent::Pause | MediaControlEvent::Toggle => {
                    Some(UserCommand::Player(PlayerCommand::TogglePlay))
                }
                MediaControlEvent::Next => Some(UserCommand::Player(PlayerCommand::Next)),
                MediaControlEvent::Previous => Some(UserCommand::Player(PlayerCommand::Prev)),
                MediaControlEvent::Stop => Some(UserCommand::Player(PlayerCommand::Stop)),
                MediaControlEvent::SetVolume(volume) => Some(UserCommand::System(SystemRequest::SetVol((volume * 100.0).round() as u8))),
                _ => None,
            };
            if let Some(cmd) = cmd {
                if let Ok(guard) = media_sender.lock() {
                    if let Some(tx) = guard.as_ref() {
                        if let Err(e) = tx.try_send(cmd) {
                            warn!("Media key command dropped: {e}");
                        }
                    }
                }
            }
        }) {
            warn!("Failed to attach media controls handler: {:?}", e);
            return;
        }

        info!("Media key bindings active (MPRIS2/MediaRemote).");
        loop {
            sleep(Duration::from_secs(1));
        }
    });
    commands
}
