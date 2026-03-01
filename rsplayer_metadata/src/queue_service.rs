use std::str::FromStr;
use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc, RwLock,
};

use rand::Rng;
use sled::{Db, IVec, Tree};

use api_models::{
    common::PlaybackMode, player::Song, playlist::PlaylistPage, settings::PlaybackQueueSetting,
    state::CurrentQueueQuery,
};

use crate::{play_statistic_repository::PlayStatisticsRepository, song_repository::SongRepository};

pub struct QueueService {
    queue_db: Db,
    status_db: Tree,
    playback_mode: RwLock<PlaybackMode>,
    random_history_db: Tree,
    random_history_index: AtomicU16,
    song_repository: Arc<SongRepository>,
    statistics_repository: Arc<PlayStatisticsRepository>,
}

const CURRENT_SONG_KEY: &str = "current_song_key";

impl QueueService {
    #[must_use]
    pub fn new(
        settings: &PlaybackQueueSetting,
        song_repository: Arc<SongRepository>,
        statistics_repository: Arc<PlayStatisticsRepository>,
    ) -> Self {
        let db = sled::open(&settings.db_path).expect("Failed to open queue db");
        let status_db = db.open_tree("status").expect("Failed to open status tree");
        let random_history_db = db
            .open_tree("random_history")
            .expect("Failed to open random_history tree");
        random_history_db.clear().expect("Failed to clear random history");
        random_history_db.flush().expect("Failed to flush random history");

        let playback_mode = if let Ok(Some(mode_str)) = status_db.get("playback_mode") {
            PlaybackMode::from_str(std::str::from_utf8(&mode_str).unwrap_or("Sequential")).unwrap_or_default()
        } else {
            PlaybackMode::Sequential
        };

        Self {
            queue_db: db,
            status_db,
            playback_mode: RwLock::new(playback_mode),
            random_history_db,
            random_history_index: AtomicU16::new(0),
            song_repository,
            statistics_repository,
        }
    }

    pub fn cycle_playback_mode(&self) -> PlaybackMode {
        let mut mode_lock = self.playback_mode.write().unwrap();

        let modes: Vec<_> = PlaybackMode::all();
        let current_index = modes.iter().position(|&m| m == *mode_lock).unwrap_or(0);
        let next_mode = modes[(current_index + 1) % modes.len()];
        *mode_lock = next_mode;

        let mode_str: &'static str = next_mode.into();
        _ = self.status_db.insert("playback_mode", mode_str);
        _ = self.status_db.flush();
        next_mode
    }

    pub fn get_playback_mode(&self) -> PlaybackMode {
        *self.playback_mode.read().unwrap()
    }

    pub fn get_current_song(&self) -> Option<Song> {
        if let Some(current_key) = self.get_current_or_first_song_key() {
            if let Ok(Some(value)) = self.queue_db.get(current_key) {
                let mut song = Song::bytes_to_song(&value).expect("Failed to parse song");
                song.statistics = self.statistics_repository.find_by_id(song.file.as_str());
                return Some(song);
            }
        }
        None
    }

    fn get_priority_queue(&self) -> Vec<IVec> {
        self.status_db
            .get("priority_queue")
            .ok()
            .flatten()
            .map(|bytes| {
                let vec_u8: Vec<Vec<u8>> = serde_json::from_slice(&bytes).unwrap_or_default();
                vec_u8.into_iter().map(IVec::from).collect()
            })
            .unwrap_or_default()
    }

    fn save_priority_queue(&self, queue: &[IVec]) {
        let vec_u8: Vec<&[u8]> = queue.iter().map(|iv| &**iv).collect();
        if let Ok(bytes) = serde_json::to_vec(&vec_u8) {
            _ = self.status_db.insert("priority_queue", bytes);
        }
    }

    pub fn add_to_priority_queue(&self, key: IVec) {
        let mut q = self.get_priority_queue();
        if let Some(pos) = q.iter().position(|k| k == &key) {
            q.remove(pos);
        }
        q.insert(0, key);
        self.save_priority_queue(&q);
    }

    #[allow(clippy::branches_sharing_code)]
    pub fn move_current_to_next_song(&self) -> bool {
        let mut pq = self.get_priority_queue();
        while !pq.is_empty() {
            let key = pq.remove(0);
            if self.queue_db.contains_key(&key).unwrap_or(false) {
                // Save the drained priority queue once, after finding a valid entry.
                self.save_priority_queue(&pq);
                _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
                return true;
            }
        }
        // All entries were stale — persist the now-empty priority queue once.
        self.save_priority_queue(&pq);

        let queue_len = self.queue_db.len();
        if queue_len == 0 {
            return false;
        }

        let mode = self.get_playback_mode();
        match mode {
            PlaybackMode::LoopSingle => {
                // Keep current song
                self.get_current_or_first_song_key().is_some()
            }
            PlaybackMode::Random => {
                if queue_len < 2 {
                    return false;
                }
                let mut rnd = rand::rng();
                let rand_position = rnd.random_range(0..queue_len - 1);
                let Some(Ok(rand_key)) = self.queue_db.iter().nth(rand_position) else {
                    return false;
                };
                _ = self.status_db.insert(CURRENT_SONG_KEY, &rand_key.0);
                let ridx = self.random_history_index.fetch_add(1, Ordering::Relaxed) + 1;
                _ = self.random_history_db.insert(ridx.to_ne_bytes(), rand_key.0);
                true
            }
            PlaybackMode::LoopQueue => {
                let Some(current_key) = self.get_current_or_first_song_key() else {
                    return false;
                };

                if let Ok(Some(next)) = self.queue_db.get_gt(&current_key) {
                    _ = self.status_db.insert(CURRENT_SONG_KEY, next.0);
                    true
                } else if let Ok(Some(first)) = self.queue_db.first() {
                    // Wrap around
                    _ = self.status_db.insert(CURRENT_SONG_KEY, first.0);
                    true
                } else {
                    false
                }
            }
            PlaybackMode::Sequential => {
                let Some(current_key) = self.get_current_or_first_song_key() else {
                    return false;
                };

                if let Ok(Some(next)) = self.queue_db.get_gt(current_key) {
                    _ = self.status_db.insert(CURRENT_SONG_KEY, next.0);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn move_current_to_previous_song(&self) -> bool {
        let mode = self.get_playback_mode();

        // If Random, use history if available
        if mode == PlaybackMode::Random {
            let ridx = self.random_history_index.load(Ordering::Relaxed);
            if ridx > 0 {
                let ridx = ridx - 1;
                let Ok(Some(prev)) = self.random_history_db.get(ridx.to_ne_bytes()) else {
                    return false;
                };
                self.random_history_index.store(ridx, Ordering::Relaxed);
                _ = self.status_db.insert(CURRENT_SONG_KEY, prev);
                return true;
            }
        }

        // For all other modes (or if random history is empty), move to previous in queue
        // Or should LoopSingle/LoopQueue behave differently?
        // Typically "Previous" button moves to previous song in list even in Loop mode.

        let Some(current_key) = self.get_current_or_first_song_key() else {
            return false;
        };

        if let Ok(Some(prev_entry)) = self.queue_db.get_lt(&current_key) {
            _ = self.status_db.insert(CURRENT_SONG_KEY, prev_entry.0);
            return true;
        } else if mode == PlaybackMode::LoopQueue {
            // Wrap around to last
            if let Ok(Some(last)) = self.queue_db.last() {
                _ = self.status_db.insert(CURRENT_SONG_KEY, last.0);
                return true;
            }
        }

        false
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
            song.file == song_id
        })
    }

    pub fn add_song(&self, song: &Song) {
        let key = self.queue_db.generate_id().unwrap().to_be_bytes();
        self.queue_db
            .insert(key, song.to_json_string_bytes())
            .expect("Failed to add song to the queue database");
    }
    pub fn add_song_by_id(&self, song_id: &str) {
        self.song_repository.find_by_id(song_id).as_ref().map_or_else(
            || {
                if song_id.starts_with("http") {
                    self.add_song(&Song {
                        file: song_id.to_string(),
                        ..Default::default()
                    });
                }
            },
            |song| {
                self.add_song(song);
            },
        );
    }

    fn get_current_or_first_song_key(&self) -> Option<IVec> {
        if let Ok(Some(result)) = self.status_db.get(CURRENT_SONG_KEY) {
            return Some(result);
        }
        let Ok(Some(first)) = self.queue_db.first() else {
            return None;
        };
        _ = self.status_db.insert(CURRENT_SONG_KEY, &first.0);
        Some(first.0)
    }

    pub fn replace_all(&self, iter: impl Iterator<Item = Song>) {
        _ = self.queue_db.clear();
        _ = self.status_db.remove(CURRENT_SONG_KEY);
        _ = self.status_db.remove("priority_queue");
        iter.for_each(|song| {
            let key = self.queue_db.generate_id().unwrap().to_be_bytes();
            _ = self.queue_db.insert(key, song.to_json_string_bytes());
        });
        // Flush so sled can reclaim space from the cleared log entries.
        _ = self.queue_db.flush();
    }

    pub fn get_queue_page<F>(&self, offset: usize, limit: usize, song_filter: F) -> (usize, Vec<Song>)
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
            .map_or_else(|| self.get_current_or_first_song_key().unwrap(), |entry| entry.0);
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
        _ = self.status_db.remove("priority_queue");
        _ = self.queue_db.flush();
    }
    pub fn query_current_queue(&self, query: CurrentQueueQuery) -> Option<PlaylistPage> {
        let mut pc = None;
        let page_size = 100;
        match query {
            CurrentQueueQuery::WithSearchTerm(term, offset) => {
                let (total, songs) = self.get_queue_page(offset, page_size, |song| {
                    if term.len() > 2 {
                        song.all_text().to_lowercase().contains(&term.to_lowercase())
                    } else {
                        true
                    }
                });
                let page = PlaylistPage {
                    total,
                    offset: offset + page_size,
                    limit: page_size,
                    items: songs,
                };
                pc = Some(page);
            }
            CurrentQueueQuery::CurrentSongPage => {
                let songs = self.get_queue_page_starting_from_current_song(page_size);
                let page = PlaylistPage {
                    total: page_size,
                    offset: 0,
                    limit: page_size,
                    items: songs,
                };
                pc = Some(page);
            }

            CurrentQueueQuery::IgnoreSongs => {}
        }
        pc
    }

    pub fn set_current_to_last(&self) {
        if let Ok(Some((key, _))) = self.queue_db.last() {
            _ = self.status_db.insert(CURRENT_SONG_KEY, key);
        }
    }

    pub fn add_songs_from_dir(&self, dir: &str) {
        self.song_repository
            .get_all_iterator()
            .filter(|item| item.file.starts_with(dir))
            .for_each(|song| {
                self.add_song(&song);
            });
    }

    pub fn add_songs_after_current(&self, songs: Vec<Song>) -> Option<IVec> {
        if songs.is_empty() {
            return None;
        }

        // Append new songs to the end of the queue (avoids O(N) rewrites of existing
        // entries). The priority queue ensures they play immediately after the current song.
        let mut new_keys: Vec<IVec> = Vec::with_capacity(songs.len());
        for song in songs {
            if let Ok(id) = self.queue_db.generate_id() {
                let key = IVec::from(&id.to_be_bytes());
                _ = self.queue_db.insert(&key, song.to_json_string_bytes());
                new_keys.push(key);
            }
        }

        // Add to priority queue in reverse order so the first song plays first.
        for key in new_keys.iter().rev() {
            self.add_to_priority_queue(key.clone());
        }

        new_keys.into_iter().next()
    }

    pub fn add_songs_from_dir_after_current(&self, dir: &str) -> Option<IVec> {
        let songs: Vec<Song> = self
            .song_repository
            .get_all_iterator()
            .filter(|item| item.file.starts_with(dir))
            .collect();

        self.add_songs_after_current(songs)
    }

    pub fn set_current_song(&self, key: IVec) {
        _ = self.status_db.insert(CURRENT_SONG_KEY, key);
    }

    pub fn add_song_after_current(&self, song_id: &str) {
        if let Some(song) = self.song_repository.find_by_id(song_id) {
            // Append to the end of the queue and schedule via the priority queue.
            // This avoids O(N) rewrites of every entry after the insertion point.
            if let Ok(id) = self.queue_db.generate_id() {
                let key = IVec::from(&id.to_be_bytes());
                _ = self.queue_db.insert(&key, song.to_json_string_bytes());
                self.add_to_priority_queue(key);
            }
        }
    }

    pub fn move_item_after_current(&self, from_index: usize) {
        let keys: Vec<IVec> = self.queue_db.iter().filter_map(Result::ok).map(|(k, _)| k).collect();

        if from_index >= keys.len() {
            return;
        }

        let current_key_opt = self.get_current_or_first_song_key();
        let current_index = current_key_opt
            .and_then(|ck| keys.iter().position(|k| k == &ck))
            .unwrap_or(0);

        if from_index == current_index || from_index == current_index + 1 {
            return;
        }

        let target_index = if from_index < current_index {
            current_index
        } else {
            current_index + 1
        };

        self.move_item(from_index, target_index);
        // The song is now at the key that was previously at target_index
        self.add_to_priority_queue(keys[target_index].clone());
    }

    pub fn move_item(&self, from_index: usize, to_index: usize) {
        let entries: Vec<(IVec, IVec)> = self.queue_db.iter().filter_map(Result::ok).collect();

        let keys: Vec<IVec> = entries.iter().map(|(k, _)| k.clone()).collect();
        let mut values: Vec<IVec> = entries.into_iter().map(|(_, v)| v).collect();

        if from_index >= keys.len() || to_index >= keys.len() {
            return;
        }

        // Update Current Song Key
        let current_key_opt = self.get_current_or_first_song_key();
        let current_index_opt = current_key_opt.and_then(|ck| keys.iter().position(|k| k == &ck));

        let val = values.remove(from_index);
        values.insert(to_index, val);

        let min_index = std::cmp::min(from_index, to_index);
        let max_index = std::cmp::max(from_index, to_index);

        for i in min_index..=max_index {
            _ = self.queue_db.insert(&keys[i], values[i].clone());
        }

        if let Some(current_index) = current_index_opt {
            let new_current_index = if current_index == from_index {
                to_index
            } else if from_index < current_index && to_index >= current_index {
                current_index - 1
            } else if from_index > current_index && to_index <= current_index {
                current_index + 1
            } else {
                current_index
            };

            if new_current_index != current_index {
                _ = self.status_db.insert(CURRENT_SONG_KEY, &keys[new_current_index]);
            }
        }
    }

    pub fn load_songs_from_dir(&self, dir: &str) {
        self.replace_all(
            self.song_repository
                .get_all_iterator()
                .filter(|item| item.file.starts_with(dir)),
        );
    }
}
