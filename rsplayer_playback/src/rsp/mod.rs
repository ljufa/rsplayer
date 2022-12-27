use std::{
    ops::Deref,
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
    thread::JoinHandle,
    vec,
};

use api_models::{
    playlist::{Playlist, PlaylistType, Playlists},
    settings::PlaybackQueueSetting,
    state::{PlayerInfo, PlayerState, SongProgress},
};
use log::{debug, error, info};
use mockall_double::double;

use ::symphonia::core::errors::Result;
#[double]
use rsplayer_metadata::metadata::MetadataService;

use crate::Player;

use self::{queue::PlaybackQueue, symphonia::SymphoniaPlayer};
mod output;
pub mod playlist;
pub mod queue;
mod symphonia;

pub enum PlayerCmd {}
pub enum PlayerEvt {}
pub struct RsPlayer {
    queue: PlaybackQueue,
    metadata_service: Arc<MetadataService>,
    tx_cmd: Sender<PlayerCmd>,
    symphonia_player: SymphoniaPlayer,
    play_handle: Vec<JoinHandle<()>>,
}
impl RsPlayer {
    pub fn new(metadata_service: Arc<MetadataService>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        RsPlayer {
            queue: PlaybackQueue::new(&PlaybackQueueSetting::default()),
            metadata_service,
            tx_cmd: tx,
            symphonia_player: SymphoniaPlayer::new(rx),
            play_handle: vec![],
        }
    }

    fn await_playing_song_to_finish(&mut self) {
        self.play_handle.drain(..).for_each(|r| r.join().unwrap());
    }
}

impl Player for RsPlayer {
    fn play_current_song(&mut self) {
        if let Some(next) = self.queue.get_current_song() {
            debug!("Playing song {:?}", next);
            self.play_handle
                .push(self.symphonia_player.play_file(next.file));
        }
    }

    fn pause_current_song(&mut self) {
        self.stop_current_song();
    }

    fn play_next_song(&mut self) {
        self.stop_current_song();
        self.queue.move_current_to_next_song();
        self.play_current_song();
    }

    fn play_prev_song(&mut self) {
        // todo!()
    }

    fn stop_current_song(&mut self) {
        self.symphonia_player.stop_playing();
        self.await_playing_song_to_finish();
    }

    fn seek_current_song(&mut self, _seconds: i8) {
        // todo!()
    }

    fn play_song(&mut self, _id: String) {
        // todo!()
    }

    fn get_current_song(&mut self) -> Option<api_models::player::Song> {
        self.queue.get_current_song()
    }

    fn load_playlist_in_queue(&mut self, playlist_id: String) {
        if &playlist_id == "RSP::Static::All" {
            let all_iter = self.metadata_service.get_all_songs_iterator();
            self.queue.replace_all(all_iter);
        }
    }

    fn load_album_in_queue(&mut self, _album_id: String) {
        // todo!()
    }

    fn load_song_in_queue(&mut self, _song_id: String) {
        // todo!()
    }

    fn remove_song_from_queue(&mut self, _id: String) {
        // todo!()
    }

    fn add_song_in_queue(&mut self, song_id: String) {
        if let Some(song) = self.metadata_service.get_song(&song_id) {
            self.queue.add(song)
        }
    }

    fn clear_queue(&mut self) {
        // todo!()
    }

    fn get_playlist_categories(&mut self) -> Vec<api_models::playlist::Category> {
        // todo!()
        vec![]
    }

    fn get_static_playlists(&mut self) -> api_models::playlist::Playlists {
        // todo!()
        Playlists {
            items: vec![PlaylistType::Saved(Playlist {
                name: "All songs".to_string(),
                id: "RSP::Static::All".to_string(),
                description: None,
                image: None,
                owner_name: None,
            })],
        }
    }

    fn get_dynamic_playlists(
        &mut self,
        _category_ids: Vec<String>,
        _offset: u32,
        _limit: u32,
    ) -> Vec<api_models::playlist::DynamicPlaylistsPage> {
        // todo!()
        vec![]
    }

    fn get_playlist_items(&mut self, _playlist_id: String) -> Vec<api_models::player::Song> {
        vec![]
    }

    fn save_queue_as_playlist(&mut self, _playlist_name: String) {
        // todo!()
    }

    fn get_player_info(&mut self) -> Option<api_models::state::PlayerInfo> {
        if self.symphonia_player.is_playing() {
            Some(PlayerInfo {
                state: Some(PlayerState::PLAYING),
                ..Default::default()
            })
        } else {
            Some(PlayerInfo {
                state: Some(PlayerState::PAUSED),
                ..Default::default()
            })
        }
    }

    fn get_playing_context(
        &mut self,
        _query: api_models::state::PlayingContextQuery,
    ) -> Option<api_models::state::PlayingContext> {
        None
    }

    fn get_song_progress(&mut self) -> api_models::state::SongProgress {
        SongProgress::default()
    }

    fn toggle_random_play(&mut self) {
        // todo!()
    }

    fn shutdown(&mut self) {
        // todo!()
    }

    fn rescan_metadata(&mut self) {
        // todo!()
    }
}

#[cfg(test)]
mod test {
    use std::{env, path::Path, sync::mpsc::channel};

    use super::*;
    use crate::Player;
    use api_models::{player::Song, settings::PlaybackQueueSetting};

    #[test]
    fn should_play_all_songs_in_queue() {
        let mut player = create_player();
        player.add_song_in_queue("mp3".to_owned());
        player.add_song_in_queue("wav".to_owned());
        player.add_song_in_queue("flac".to_owned());
        player.play_current_song();
        player.await_playing_song_to_finish();
    }

    fn create_player() -> RsPlayer {
        let ctx = Context::default();
        let mut ms = MetadataService::default();
        ms.expect_get_song().returning(|song_id| {
            let result = match song_id {
                "flac" => Song {
                    file: "assets/music.flac".to_string(),
                    id: song_id.to_string(),
                    ..Default::default()
                },
                "mp3" => Song {
                    file: "assets/music.mp3".to_string(),
                    id: song_id.to_string(),
                    ..Default::default()
                },
                "wav" => Song {
                    file: "assets/music.wav".to_string(),
                    id: song_id.to_string(),
                    ..Default::default()
                },
                _ => panic!("Unsupported"),
            };
            Some(result)
        });

        let (tx, rx) = channel();
        RsPlayer {
            metadata_service: Arc::new(ms),
            queue: PlaybackQueue::new(&PlaybackQueueSetting {
                db_path: ctx.db_dir.clone(),
            }),
            tx_cmd: tx,
            symphonia_player: SymphoniaPlayer::new(rx),
            play_handle: vec![],
        }
    }

    pub struct Context {
        pub db_dir: String,
    }

    impl Default for Context {
        fn default() -> Self {
            _ = env_logger::builder().is_test(true).try_init();
            let path = env::current_dir().unwrap();
            info!("Current directory is {}", path.display());
            let rnd = random_string::generate(6, "utf8");
            Self {
                db_dir: format!("/tmp/queue{rnd}"),
            }
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            let path = &self.db_dir;
            if Path::new(path).exists() {
                _ = std::fs::remove_dir_all(path);
            }
        }
    }
}
