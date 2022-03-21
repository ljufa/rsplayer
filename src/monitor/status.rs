use crate::audio_device::alsa::AudioCard;
use crate::player::PlayerFactory;
use api_models::player::StatusChangeEvent;

use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::{Receiver, Sender};

pub async fn monitor(
    player_factory: Arc<Mutex<PlayerFactory>>,
    state_changes_tx: Sender<StatusChangeEvent>,
    mut state_changes_rx: Receiver<StatusChangeEvent>,
    audio_card: Arc<AudioCard>,
) {
    let mut last_track_info = None;
    let mut last_player_info = None;
    info!("Monitor thread started.");
    loop {
        if let Ok(StatusChangeEvent::Shutdown) = state_changes_rx.try_recv() {
            info!("Program terminate, monitoring stopped.");
            break;
        }

        if audio_card.is_device_in_use() {
            let new_track_info = player_factory
                .lock()
                .unwrap()
                .get_current_player()
                .get_current_track_info();
            trace!(
                "new track info:\t{:?}\nlast track info:\t{:?}",
                new_track_info,
                last_track_info
            );

            if last_track_info != new_track_info {
                if let Some(new) = new_track_info.as_ref() {
                    state_changes_tx.send(StatusChangeEvent::CurrentTrackInfoChanged(new.clone()));
                } else {
                    debug!("Current track info in None");
                }
                last_track_info = new_track_info;
            }
        }
        let new_player_info = player_factory
            .lock()
            .unwrap()
            .get_current_player()
            .get_player_info();
        if last_player_info != new_player_info {
            if let Some(new_p_info) = new_player_info.as_ref() {
                state_changes_tx.send(StatusChangeEvent::PlayerInfoChanged(new_p_info.clone()));
            }
            last_player_info = new_player_info;
        }
    }
}
