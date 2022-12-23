use std::sync::Arc;

use mockall_double::double;

#[double]
use rsplayer_metadata::metadata::MetadataService;

use crate::Player;

use self::queue::PlaybackQueue;
mod output;
mod play;
pub mod queue;

pub struct RsPlayer {
    pub queue: PlaybackQueue,
    pub metadata_service: Arc<MetadataService>,
}
impl Player for RsPlayer {
    fn play_current_song(&mut self) {
        if let Some(current_song) = self.queue.get_current_song() {
            play::play_file(&current_song.file);
        }
    }

    fn pause_current_song(&mut self) {
        todo!()
    }

    fn play_next_song(&mut self) {
        todo!()
    }

    fn play_prev_song(&mut self) {
        todo!()
    }

    fn stop_current_song(&mut self) {
        todo!()
    }

    fn seek_current_song(&mut self, seconds: i8) {
        todo!()
    }

    fn play_song(&mut self, id: String) {
        todo!()
    }

    fn get_current_song(&mut self) -> Option<api_models::player::Song> {
        self.queue.get_current_song()
    }

    fn load_playlist_in_queue(&mut self, pl_id: String) {
        todo!()
    }

    fn load_album_in_queue(&mut self, album_id: String) {
        todo!()
    }

    fn load_song_in_queue(&mut self, song_id: String) {
        todo!()
    }

    fn remove_song_from_queue(&mut self, id: String) {
        todo!()
    }

    fn add_song_in_queue(&mut self, song_id: String) {
        if let Some(song) = self.metadata_service.get_song(&song_id) {
            self.queue.add(song)
        }
    }

    fn clear_queue(&mut self) {
        todo!()
    }

    fn get_playlist_categories(&mut self) -> Vec<api_models::playlist::Category> {
        todo!()
    }

    fn get_static_playlists(&mut self) -> api_models::playlist::Playlists {
        todo!()
    }

    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<api_models::playlist::DynamicPlaylistsPage> {
        todo!()
    }

    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<api_models::player::Song> {
        todo!()
    }

    fn save_queue_as_playlist(&mut self, playlist_name: String) {
        todo!()
    }

    fn get_player_info(&mut self) -> Option<api_models::state::PlayerInfo> {
        todo!()
    }

    fn get_playing_context(
        &mut self,
        query: api_models::state::PlayingContextQuery,
    ) -> Option<api_models::state::PlayingContext> {
        todo!()
    }

    fn get_song_progress(&mut self) -> api_models::state::SongProgress {
        todo!()
    }

    fn toggle_random_play(&mut self) {
        todo!()
    }

    fn shutdown(&mut self) {
        todo!()
    }

    fn rescan_metadata(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Player;
    use api_models::{player::Song, settings::PlaybackQueueSetting};

    #[test]
    fn test_add_song_in_queue() {
        const FILE_PATH: &str = "assets/music.flac";
        let mut ms = MetadataService::default();
        ms.expect_get_song().return_once(|_| {
            Some(Song {
                file: FILE_PATH.to_string(),
                ..Song::default()
            })
        });
        let mut player = RsPlayer {
            metadata_service: Arc::new(ms),
            queue: PlaybackQueue::new(&PlaybackQueueSetting {
                db_path: "/tmp/queue.db".to_string(),
            }),
        };
        player.add_song_in_queue(FILE_PATH.to_string());
        assert_eq!(
            player.get_current_song().unwrap().file,
            FILE_PATH.to_string()
        );
    }
}
