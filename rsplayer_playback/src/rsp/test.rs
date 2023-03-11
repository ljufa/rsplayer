use std::{env, path::Path};

use super::*;
use crate::{Player, CATEGORY_ID_BY_FOLDER};
use api_models::{player::Song, settings::PlaybackQueueSetting};
use log::info;


#[test]
fn test_get_dynamic_pl(){
    let player = create_player();
    let dpl = player.get_dynamic_playlists(vec![CATEGORY_ID_BY_FOLDER.to_owned()], 0, 10);
    assert_eq!(dpl[0].category_id, CATEGORY_ID_BY_FOLDER);
}

#[test]
fn should_play_radio_url() {
    let player = create_player();
    player.add_song_in_queue(
        "https://fluxmusic.api.radiosphere.io/channels/90s/stream.aac?quality=10",
    );
    player.add_song_in_queue("https://stream.rcast.net/66036");
    player.play_from_current_queue_song();
    std::thread::sleep(Duration::from_secs(10));
    player.play_next_song();
    std::thread::sleep(Duration::from_secs(10));
    player.stop_current_song();
    player.await_playing_song_to_finish();
}
#[test]
fn should_play_all_songs_in_queue() {
    let player = create_player();
    player.add_song_in_queue("mp3");
    player.add_song_in_queue("flac");
    player.play_from_current_queue_song();
    assert!(player.await_playing_song_to_finish()[0].is_ok());
}

fn create_player() -> RsPlayer {
    let ctx = Context::default();
    let mut ms = MetadataService::default();
    ms.expect_find_song_by_id().returning(|song_id| {
        Some(Song {
            file: if song_id.starts_with("http") {
                song_id.to_string()
            } else {
                format!("music.{song_id}")
            },
            id: song_id.to_string(),
            ..Default::default()
        })
    });
    ms.expect_get_all_songs_iterator().returning(|| {
        Box::new(vec![Song::default(), Song::default()].into_iter())
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
        symphonia_player: SymphoniaPlayer::new(
            queue,
            "plughw:CARD=PCH,DEV=0".to_string(),
            1,
            "../rsplayer_metadata/assets".to_string(),
        ),
        play_handle: Arc::new(Mutex::new(vec![])),
        music_dir_depth: 0,
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
