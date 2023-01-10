
use std::{env, path::Path};

use super::*;
use crate::Player;
use api_models::{player::Song, settings::PlaybackQueueSetting};
use log::info;

#[test]
fn should_play_all_songs_in_queue() {
    let player = create_player();
    player.add_song_in_queue("mp3");
    player.add_song_in_queue("flac");
    player.play_queue_from_current_song();
    assert!(player.await_playing_song_to_finish()[0].is_ok());
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

    let queue = Arc::new(PlaybackQueue::new(&PlaybackQueueSetting {
        db_path: ctx.db_dir.clone(),
    }));
    RsPlayer {
        metadata_service: Arc::new(ms),
        playlist_service: Arc::new(PlaylistService::new(&PlaylistSetting {
            db_path: format!("{}plista", ctx.db_dir),
        })),
        queue: queue.clone(),
        symphonia_player: SymphoniaPlayer::new(queue, "default".to_string()),
        play_handle: Arc::new(Mutex::new(vec![])),
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
