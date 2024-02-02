use api_models::state::PlayerInfo;
use api_models::state::SongProgress;
use api_models::state::StateChangeEvent;
use log::info;
use rsplayer_metadata::queue_service::QueueService;
use rsplayer_playback::rsp::PlayerService;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Sender;

pub async fn monitor(
    player: Arc<PlayerService>,
    queue_service: Arc<QueueService>,
    state_changes_tx: Sender<StateChangeEvent>,
) {
    info!("Status monitor thread started.");
    let mut last_track_info = None;
    let mut last_player_info = PlayerInfo::default();
    let mut last_query_queue_result = None;
    let mut last_progress = SongProgress {
        total_time: Duration::ZERO,
        current_time: Duration::ZERO,
    };
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        // check track info change

        let new_track_info = queue_service.get_current_song();
        if last_track_info != new_track_info {
            if let Some(new) = new_track_info.as_ref() {
                _ = state_changes_tx.send(StateChangeEvent::CurrentSongEvent(new.clone()));
            }
            last_track_info = new_track_info;
        }
        // check player info change
        let new_player_info = player.get_player_info();
        if last_player_info != new_player_info {
            _ = state_changes_tx.send(StateChangeEvent::PlayerInfoEvent(new_player_info.clone()));
            last_player_info = new_player_info;
        }
        // check progres info change
        let new_progress = player.get_song_progress();
        if last_progress != new_progress {
            _ = state_changes_tx.send(StateChangeEvent::SongTimeEvent(new_progress.clone()));
            last_progress = new_progress;
        }

        // check playing context change
        let new_query_queue_result =
            queue_service.query_current_queue(api_models::state::CurrentQueueQuery::IgnoreSongs);
        if last_query_queue_result != new_query_queue_result {
            // if let Some(new_qc) = new_query_queue_result.as_ref() {
            _ = state_changes_tx.send(StateChangeEvent::CurrentQueueEvent(new_query_queue_result.clone()));
            // }
            last_query_queue_result = new_query_queue_result;
        }
    }
}
