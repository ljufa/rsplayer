use crate::common::ArcAudioInterfaceSvc;
use crate::common::MutArcPlayerService;

use api_models::state::SongProgress;
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
    let mut last_playing_context = None;
    let mut last_progress = SongProgress {
        total_time: Duration::ZERO,
        current_time: Duration::ZERO,
    };
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        // check track info change
        let new_track_info = player_svc
            .lock()
            .unwrap()
            .get_current_player()
            .get_current_song();
        if last_track_info != new_track_info {
            if let Some(new) = new_track_info.as_ref() {
                _ = state_changes_tx.send(StateChangeEvent::CurrentSongEvent(new.clone()));
            }
            last_track_info = new_track_info;
        }
        // check player info change
        let new_player_info = player_svc
            .lock()
            .unwrap()
            .get_current_player()
            .get_player_info();
        if last_player_info != new_player_info {
            if let Some(new_p_info) = new_player_info.as_ref() {
                _ = state_changes_tx.send(StateChangeEvent::PlayerInfoEvent(new_p_info.clone()));
            }
            last_player_info = new_player_info;
        }
        // check progres info change
        let new_progress = player_svc
            .lock()
            .unwrap()
            .get_current_player()
            .get_song_progress();
        if last_progress != new_progress {
            _ = state_changes_tx.send(StateChangeEvent::SongTimeEvent(new_progress.clone()));
            last_progress = new_progress;
        }

        // check playing context change
        let new_playing_context = player_svc
            .lock()
            .unwrap()
            .get_current_player()
            .get_playing_context(false);
        if last_playing_context != new_playing_context {
            if let Some(new_pc) = new_playing_context.as_ref() {
                _ = state_changes_tx
                    .send(StateChangeEvent::CurrentPlayingContextEvent(new_pc.clone()));
            }
            last_playing_context = new_playing_context;
        }
    }
}
