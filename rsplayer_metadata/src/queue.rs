use std::sync::atomic::{AtomicBool, Ordering};

use api_models::{player::Song, settings::PlaybackQueueSetting};
use rand::Rng;
use sled::{Db, IVec, Tree};

pub struct PlaybackQueue {
    queue_db: Db,
    status_db: Tree,
    random_flag: AtomicBool,
}
const CURRENT_SONG_KEY: &str = "current_song_key";
impl PlaybackQueue {
    pub fn new(settings: &PlaybackQueueSetting) -> Self {
        let db = sled::open(&settings.db_path).expect("Failed to open queue db");
        let status_db = db.open_tree("status").expect("Failed to open status tree");
        let random_flag = status_db.contains_key("random_next").unwrap_or(false);
        Self {
            queue_db: db,
            status_db,
            random_flag: AtomicBool::new(random_flag),
        }
    }

    pub fn toggle_random_next(&self) {
        _ = self
            .random_flag
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |existing| {
                let new = !existing;
                if new {
                    _ = self.status_db.insert("random_next", "true");
                } else {
                    _ = self.status_db.remove("random_next");
                }
                Some(new)
            });
    }

    pub fn get_random_next(&self) -> bool {
        self.random_flag.load(Ordering::SeqCst)
    }

    pub fn get_current_song(&self) -> Option<Song> {
        if let Some(current_key) = self.get_current_or_first_song_key() {
            if let Ok(Some(value)) = self.queue_db.get(current_key) {
                return Song::bytes_to_song(&value);
            }
        }
        None
    }

    #[allow(clippy::branches_sharing_code)]
    pub fn move_current_to_next_song(&self) -> bool {
        if self.get_random_next() {
            let mut rnd = rand::thread_rng();
            let rand_position = rnd.gen_range(0, &self.queue_db.len() - 1);
            let Some(Ok(rand_key)) = self.queue_db.iter().nth(rand_position) else {
                return false;
            };
            _ = self.status_db.insert(CURRENT_SONG_KEY, rand_key.0);
            true
        } else {
            let Some(current_key) = self.get_current_or_first_song_key() else {
                return false;
            };

            let Ok(Some(next)) = self.queue_db.get_gt(current_key) else {
                return false;
            };
            _ = self.status_db.insert(CURRENT_SONG_KEY, next.0);
            true
        }
    }

    pub fn move_current_to_previous_song(&self) -> bool {
        let Some(current_key) = self.get_current_or_first_song_key() else {
            return false;
        };
        let Ok(Some(prev_entry)) = self.queue_db.get_lt(current_key) else{
            return false;
        };
        _ = self.status_db.insert(CURRENT_SONG_KEY, prev_entry.0);
        true
    }

    pub fn move_current_to(&self, song_id: &str) -> bool {
        let Some(entry) = self.find_entry_by_song_id(song_id) else {
            return false;
        };
        _ = self.status_db.insert(CURRENT_SONG_KEY, entry.0);
        true
    }

    pub fn remove_song(&self, song_id: &str) {
        if let Some(result) = self.find_entry_by_song_id(song_id) {
            _ = self.queue_db.remove(result.0);
        }
    }
    fn find_entry_by_song_id(&self, song_id: &str) -> Option<(IVec, IVec)> {
        self.queue_db.iter().filter_map(Result::ok).find(|entry| {
            let Some(song) = Song::bytes_to_song(&entry.1) else {
                return false;
            };
            song.id == song_id
        })
    }

    pub fn add_song(&self, song: &Song) {
        let key = self.queue_db.generate_id().unwrap().to_be_bytes();
        self.queue_db
            .insert(key, song.to_json_string_bytes())
            .expect("Failed to add song to the queue database");
    }

    fn get_current_or_first_song_key(&self) -> Option<IVec> {
        if let Ok(Some(result)) = self.status_db.get(CURRENT_SONG_KEY) {
            return Some(result);
        };
        let Ok(Some(first)) = self.queue_db.first() else {
            return None;
        };
        _ = self.status_db.insert(CURRENT_SONG_KEY, &first.0);
        Some(first.0)
    }

    pub fn replace_all(&self, iter: impl Iterator<Item = Song>) {
        _ = self.queue_db.clear();
        _ = self.status_db.remove(CURRENT_SONG_KEY);
        iter.for_each(|song| {
            let key = self.queue_db.generate_id().unwrap().to_be_bytes();
            _ = self.queue_db.insert(key, song.to_json_string_bytes());
        });
    }

    pub fn get_queue_page<F>(
        &self,
        offset: usize,
        limit: usize,
        song_filter: F,
    ) -> (usize, Vec<Song>)
    where
        F: Fn(&Song) -> bool,
    {
        let total = self.queue_db.len();
        if total == 0 {
            return (0, vec![]);
        }
        let from = self
            .queue_db
            .iter()
            .filter_map(std::result::Result::ok)
            .nth(offset)
            .map_or_else(
                || self.get_current_or_first_song_key().unwrap(),
                |entry| entry.0,
            );
        (
            total,
            self.queue_db
                .range(from.to_vec()..)
                .filter_map(std::result::Result::ok)
                .map_while(|s| Song::bytes_to_song(&s.1))
                .filter(|s| song_filter(s))
                .take(limit)
                .collect(),
        )
    }

    pub fn get_queue_page_starting_from_current_song(&self, limit: usize) -> Vec<Song> {
        self.get_current_or_first_song_key()
            .as_ref()
            .map_or_else(Vec::new, |from| {
                self.queue_db
                    .range(from.to_vec()..)
                    .filter_map(std::result::Result::ok)
                    .map_while(|s| Song::bytes_to_song(&s.1))
                    .take(limit)
                    .collect()
            })
    }

    pub fn get_all_songs(&self) -> Vec<Song> {
        self.queue_db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| Song::bytes_to_song(&s.1))
            .collect()
    }

    pub fn clear(&self) {
        _ = self.queue_db.clear();
        _ = self.status_db.remove(CURRENT_SONG_KEY);
    }
}
