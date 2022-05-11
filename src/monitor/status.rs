use crate::common::ArcAudioInterfaceSvc;
use crate::common::MutArcPlayerService;

use api_models::state::StateChangeEvent;

use std::time::Duration;
use tokio::sync::broadcast::Sender;

pub async fn monitor(
    player_svc: MutArcPlayerService,
    state_changes_tx: Sender<StateChangeEvent>,
    ai_svc: ArcAudioInterfaceSvc,
) {
    info!("Status monitor thread started.");
    let mut last_track_info = None;
    let mut last_player_info = None;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if ai_svc.is_device_in_use() {
            let new_track_info = player_svc
                .lock()
                .unwrap()
                .get_current_player()
                .get_current_song();
            trace!(
                "new track info:\t{:?}\nlast track info:\t{:?}",
                new_track_info,
                last_track_info
            );

            if last_track_info != new_track_info {
                if let Some(new) = new_track_info.as_ref() {
                    _ = state_changes_tx
                        .send(StateChangeEvent::CurrentTrackInfoChanged(new.clone()));
                } else {
                    debug!("Current track info in None");
                }
                last_track_info = new_track_info;
            }
        }
        let new_player_info = player_svc
            .lock()
            .unwrap()
            .get_current_player()
            .get_player_info();
        if last_player_info != new_player_info {
            if let Some(new_p_info) = new_player_info.as_ref() {
                _ = state_changes_tx.send(StateChangeEvent::PlayerInfoChanged(new_p_info.clone()));
            }
            last_player_info = new_player_info;
        }
    }
}
