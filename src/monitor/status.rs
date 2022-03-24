use crate::audio_device::alsa::AudioCard;
use crate::player::PlayerService;
use api_models::player::StatusChangeEvent;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast::Sender;

pub async fn monitor(
    player_factory: Arc<Mutex<PlayerService>>,
    state_changes_tx: Sender<StatusChangeEvent>,
    audio_card: Arc<AudioCard>,
) {
    info!("Status monitor thread started.");
    let mut last_track_info = None;
    let mut last_player_info = None;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
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
                    _ = state_changes_tx
                        .send(StatusChangeEvent::CurrentTrackInfoChanged(new.clone()));
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
                _ = state_changes_tx.send(StatusChangeEvent::PlayerInfoChanged(new_p_info.clone()));
            }
            last_player_info = new_player_info;
        }
    }
}
