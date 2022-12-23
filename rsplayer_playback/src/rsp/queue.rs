use api_models::{player::Song, settings::PlaybackQueueSetting};
use sled::Db;

pub struct PlaybackQueue {
    db: Db,
}

impl PlaybackQueue {
    pub fn new(settings: &PlaybackQueueSetting) -> Self {
        let db = sled::open(&settings.db_path).expect("Failed to open queue db");
        PlaybackQueue { db }
    }

    pub(crate) fn get_current_song(&self) -> Option<Song> {
        if let Ok(Some(song)) = self.db.first() {
            Song::bytes_to_song(song.1.to_vec())
        } else {
            None
        }
    }

    pub(crate) fn add(&self, song: api_models::player::Song) {
        _ = self.db.insert(&song.id, song.to_json_string_bytes());
    }
}
