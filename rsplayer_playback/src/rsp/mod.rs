use std::{sync::Arc, vec};

use api_models::{
    playlist::{Playlist, PlaylistType, Playlists},
    state::SongProgress, player::Song,
};
use log::{debug, error, info};
use mockall_double::double;

#[double]
use rsplayer_metadata::metadata::MetadataService;

use crate::Player;

use self::queue::PlaybackQueue;
mod output;
mod play;
pub mod queue;
pub mod playlist;

pub struct RsPlayer {
    pub queue: PlaybackQueue,
    pub metadata_service: Arc<MetadataService>,
}
impl RsPlayer {
    
    fn play_songs_in_queue(&mut self) {
        while let Some(next) = self.queue.get_current_song() {
            debug!("Playing song {:?}", next);
            match play::play_file(&next.file) {
                Ok(_) => {
                    info!("Play finished");
                }
                Err(e) => {
                    error!("Error:{} - Failed to play song", e);
                }
            }
            if !self.queue.move_current_to_next_song() {
                break;
            }
        }
    }
    
}

impl Player for RsPlayer {
    
    fn play_current_song(&mut self) {
        std::thread::spawn(||{
            
        });
        self.play_songs_in_queue();
    }

    fn pause_current_song(&mut self) {
        // todo!()
    }

    fn play_next_song(&mut self) {
        // todo!()
    }

    fn play_prev_song(&mut self) {
        // todo!()
    }

    fn stop_current_song(&mut self) {
        // todo!()
    }

    fn seek_current_song(&mut self, seconds: i8) {
        // todo!()
    }

    fn play_song(&mut self, id: String) {
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

    fn load_album_in_queue(&mut self, album_id: String) {
        // todo!()
    }

    fn load_song_in_queue(&mut self, song_id: String) {
        // todo!()
    }

    fn remove_song_from_queue(&mut self, id: String) {
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
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<api_models::playlist::DynamicPlaylistsPage> {
        // todo!()
        vec![]
    }

    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<api_models::player::Song> {
        if &playlist_id == "RSP::Static::All" {
            let it = self.queue.db.iter().filter_map(|f|f.ok()).map(|f|Song::bytes_to_song(f.1.to_vec()));
            self.queue.replace_all(it)
        }
        vec![]
    }

    fn save_queue_as_playlist(&mut self, playlist_name: String) {
        // todo!()
    }

    fn get_player_info(&mut self) -> Option<api_models::state::PlayerInfo> {
        None
    }

    fn get_playing_context(
        &mut self,
        query: api_models::state::PlayingContextQuery,
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
    use std::{env, path::Path};

    use super::*;
    use crate::Player;
    use api_models::{player::Song, settings::PlaybackQueueSetting};

    #[test]
    fn should_play_all_songs_in_queue() {
        let mut player = create_player();
        player.add_song_in_queue("mp3".to_owned());
        player.add_song_in_queue("wav".to_owned());
        player.add_song_in_queue("flac".to_owned());
        player.play_songs_in_queue()
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

        RsPlayer {
            metadata_service: Arc::new(ms),
            queue: PlaybackQueue::new(&PlaybackQueueSetting {
                db_path: ctx.db_dir.clone(),
            }),
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
                db_dir: format!("/tmp/queue{}", rnd),
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
