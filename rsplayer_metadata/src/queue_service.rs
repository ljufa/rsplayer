use std::collections::HashSet;
use std::ops::Bound;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicU16, AtomicU64, Ordering},
    Arc, RwLock,
};

use fjall::{Database, Keyspace, KeyspaceCreateOptions};
use rand::RngExt;

use api_models::{common::PlaybackMode, player::Song, playlist::PlaylistPage, state::CurrentQueueQuery};

use crate::{play_statistic_repository::PlayStatisticsRepository, song_repository::SongRepository};

pub struct QueueService {
    queue_db: Keyspace,
    status_db: Keyspace,
    playback_mode: RwLock<PlaybackMode>,
    random_history_db: Keyspace,
    random_history_index: AtomicU16,
    random_played_keys: RwLock<HashSet<Vec<u8>>>,
    next_id: AtomicU64,
    song_repository: Arc<SongRepository>,
    statistics_repository: Arc<PlayStatisticsRepository>,
}

const CURRENT_SONG_KEY: &str = "current_song_key";
const NEXT_ID_KEY: &str = "_next_queue_id";

impl QueueService {
    #[must_use]
    pub fn new(
        db: &Database,
        song_repository: Arc<SongRepository>,
        statistics_repository: Arc<PlayStatisticsRepository>,
    ) -> Self {
        let queue_db = db
            .keyspace("queue", KeyspaceCreateOptions::default)
            .expect("Failed to open queue keyspace");
        let status_db = db
            .keyspace("queue_status", KeyspaceCreateOptions::default)
            .expect("Failed to open queue_status keyspace");
        let random_history_db = db
            .keyspace("queue_random_history", KeyspaceCreateOptions::default)
            .expect("Failed to open queue_random_history keyspace");

        let playback_mode = if let Ok(Some(mode_bytes)) = status_db.get("playback_mode") {
            PlaybackMode::from_str(std::str::from_utf8(&mode_bytes).unwrap_or("Sequential")).unwrap_or_default()
        } else {
            PlaybackMode::Sequential
        };

        let next_id = status_db
            .get(NEXT_ID_KEY)
            .ok()
            .flatten()
            .and_then(|b| {
                let arr: [u8; 8] = b.as_ref().try_into().ok()?;
                Some(u64::from_be_bytes(arr))
            })
            .unwrap_or(0);

        let service = Self {
            queue_db,
            status_db,
            playback_mode: RwLock::new(playback_mode),
            random_history_db,
            random_history_index: AtomicU16::new(0),
            random_played_keys: RwLock::new(HashSet::new()),
            next_id: AtomicU64::new(next_id),
            song_repository,
            statistics_repository,
        };
        service.reset_random_state();
        service
    }

    fn generate_id(&self) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id.is_multiple_of(64) {
            _ = self.status_db.insert(NEXT_ID_KEY, (id + 64).to_be_bytes());
        }
        id
    }

    fn reset_random_state(&self) {
        let keys: Vec<Vec<u8>> = self
            .random_history_db
            .iter()
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
        for key in keys {
            _ = self.random_history_db.remove(key);
        }
        self.random_history_index.store(0, Ordering::Relaxed);
        self.random_played_keys.write().expect("lock poisoned").clear();
    }

    pub fn cycle_playback_mode(&self) -> PlaybackMode {
        let mut mode_lock = self.playback_mode.write().expect("lock poisoned");
        let modes: Vec<_> = PlaybackMode::all();
        let current_index = modes.iter().position(|&m| m == *mode_lock).unwrap_or(0);
        let next_mode = modes[(current_index + 1) % modes.len()];
        *mode_lock = next_mode;
        let mode_str: &'static str = next_mode.into();
        _ = self.status_db.insert("playback_mode", mode_str);
        self.reset_random_state();
        next_mode
    }

    pub fn get_playback_mode(&self) -> PlaybackMode {
        *self.playback_mode.read().expect("lock poisoned")
    }

    pub fn get_current_song(&self) -> Option<Song> {
        let current_key = self.get_current_or_first_song_key()?;
        let value = self.queue_db.get(&current_key).ok()??;
        let mut song = Song::bytes_to_song(&value)?;
        song.statistics = self.statistics_repository.find_by_id(song.file.as_str());
        Some(song)
    }

    fn get_priority_queue(&self) -> Vec<Vec<u8>> {
        self.status_db
            .get("priority_queue")
            .ok()
            .flatten()
            .map(|bytes| serde_json::from_slice::<Vec<Vec<u8>>>(&bytes).unwrap_or_default())
            .unwrap_or_default()
    }

    fn save_priority_queue(&self, queue: &[Vec<u8>]) {
        let vec_u8: Vec<&[u8]> = queue.iter().map(std::vec::Vec::as_slice).collect();
        if let Ok(bytes) = serde_json::to_vec(&vec_u8) {
            _ = self.status_db.insert("priority_queue", bytes);
        }
    }

    pub fn add_to_priority_queue(&self, key: Vec<u8>) {
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
                self.save_priority_queue(&pq);
                _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
                return true;
            }
        }
        self.save_priority_queue(&pq);

        let mode = self.get_playback_mode();
        match mode {
            PlaybackMode::LoopSingle => self.get_current_or_first_song_key().is_some(),
            PlaybackMode::Random => {
                let all_keys: Vec<Vec<u8>> = self
                    .queue_db
                    .iter()
                    .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
                    .collect();
                if all_keys.len() < 2 {
                    return false;
                }
                let current_key = self.get_current_or_first_song_key();
                let mut played = self.random_played_keys.write().expect("lock poisoned");
                // Mark current song as played
                if let Some(ref ck) = current_key {
                    played.insert(ck.clone());
                }
                let unplayed: Vec<&Vec<u8>> = all_keys.iter().filter(|k| !played.contains(*k)).collect();
                let candidates = if unplayed.is_empty() {
                    // All songs played — reset but exclude current to avoid repeat
                    played.clear();
                    if let Some(ref ck) = current_key {
                        played.insert(ck.clone());
                    }
                    all_keys
                        .iter()
                        .filter(|k| current_key.as_ref() != Some(*k))
                        .collect::<Vec<_>>()
                } else {
                    unplayed
                };
                if candidates.is_empty() {
                    return false;
                }
                let mut rnd = rand::rng();
                let rand_position = rnd.random_range(0..candidates.len());
                let rand_key = candidates[rand_position].clone();
                played.insert(rand_key.clone());
                _ = self.status_db.insert(CURRENT_SONG_KEY, &rand_key);
                let ridx = self.random_history_index.fetch_add(1, Ordering::Relaxed) + 1;
                _ = self.random_history_db.insert(ridx.to_ne_bytes(), &rand_key);
                true
            }
            PlaybackMode::LoopQueue => {
                let Some(current_key) = self.get_current_or_first_song_key() else {
                    return false;
                };
                self.queue_db
                    .range::<&[u8], _>((Bound::Excluded(current_key.as_slice()), Bound::Unbounded))
                    .next()
                    .or_else(|| self.queue_db.first_key_value())
                    .is_some_and(|guard| {
                        let key = guard.key().expect("Failed to get key").to_vec();
                        _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
                        true
                    })
            }
            PlaybackMode::Sequential => {
                let Some(current_key) = self.get_current_or_first_song_key() else {
                    return false;
                };
                self.queue_db
                    .range::<&[u8], _>((Bound::Excluded(current_key.as_slice()), Bound::Unbounded))
                    .next()
                    .is_some_and(|guard| {
                        let key = guard.key().expect("Failed to get key").to_vec();
                        _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
                        true
                    })
            }
        }
    }

    pub fn move_current_to_previous_song(&self) -> bool {
        let mode = self.get_playback_mode();
        if mode == PlaybackMode::Random {
            let ridx = self.random_history_index.load(Ordering::Relaxed);
            if ridx > 0 {
                let ridx = ridx - 1;
                let Ok(Some(prev)) = self.random_history_db.get(ridx.to_ne_bytes()) else {
                    return false;
                };
                self.random_history_index.store(ridx, Ordering::Relaxed);
                _ = self.status_db.insert(CURRENT_SONG_KEY, prev.as_ref());
                return true;
            }
        }

        let Some(current_key) = self.get_current_or_first_song_key() else {
            return false;
        };

        // get_lt: range before current, reversed
        if let Some(guard) = self.queue_db.range(..current_key.as_slice()).next_back() {
            let key = guard.key().expect("Failed to get key").to_vec();
            _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
            return true;
        } else if mode == PlaybackMode::LoopQueue {
            if let Some(guard) = self.queue_db.last_key_value() {
                let key = guard.key().expect("Failed to get key").to_vec();
                _ = self.status_db.insert(CURRENT_SONG_KEY, &key);
                return true;
            }
        }
        false
    }

    pub fn move_current_to(&self, song_id: &str) -> bool {
        let Some(entry) = self.find_entry_by_song_id(song_id) else {
            return false;
        };
        _ = self.status_db.insert(CURRENT_SONG_KEY, &entry.0);
        true
    }

    pub fn remove_song(&self, song_id: &str) {
        if let Some(result) = self.find_entry_by_song_id(song_id) {
            _ = self.queue_db.remove(result.0);
        }
    }

    fn find_entry_by_song_id(&self, song_id: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        self.queue_db.iter().find_map(|guard| {
            let (key, value) = guard.into_inner().ok()?;
            let song = Song::bytes_to_song(&value)?;
            if song.file == song_id {
                Some((key.to_vec(), value.to_vec()))
            } else {
                None
            }
        })
    }

    pub fn add_song(&self, song: &Song) {
        let key = self.generate_id().to_be_bytes();
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

    fn get_current_or_first_song_key(&self) -> Option<Vec<u8>> {
        if let Ok(Some(result)) = self.status_db.get(CURRENT_SONG_KEY) {
            return Some(result.to_vec());
        }
        let guard = self.queue_db.first_key_value()?;
        let first_key = guard.key().ok()?.to_vec();
        _ = self.status_db.insert(CURRENT_SONG_KEY, &first_key);
        Some(first_key)
    }

    pub fn replace_all(&self, iter: impl Iterator<Item = Song>) {
        let keys: Vec<Vec<u8>> = self
            .queue_db
            .iter()
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
        for key in keys {
            _ = self.queue_db.remove(key);
        }
        _ = self.status_db.remove(CURRENT_SONG_KEY);
        _ = self.status_db.remove("priority_queue");
        self.reset_random_state();
        iter.for_each(|song| {
            let key = self.generate_id().to_be_bytes();
            _ = self.queue_db.insert(key, song.to_json_string_bytes());
        });
    }

    pub fn get_queue_page<F>(&self, offset: usize, limit: usize, song_filter: F) -> (usize, Vec<Song>)
    where
        F: Fn(&Song) -> bool,
    {
        let total = self.queue_db.approximate_len();
        if total == 0 {
            return (0, vec![]);
        }
        let Some(from) = self
            .queue_db
            .iter()
            .nth(offset)
            .and_then(|guard| guard.key().ok().map(|k| k.to_vec()))
            .or_else(|| self.get_current_or_first_song_key())
        else {
            return (0, vec![]);
        };
        (
            total,
            self.queue_db
                .range(from.as_slice()..)
                .filter_map(|guard| {
                    let value = guard.value().ok()?;
                    Song::bytes_to_song(&value)
                })
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
                    .range(from.as_slice()..)
                    .filter_map(|guard| {
                        let value = guard.value().ok()?;
                        Song::bytes_to_song(&value)
                    })
                    .take(limit)
                    .collect()
            })
    }

    pub fn get_all_songs(&self) -> Vec<Song> {
        self.queue_db
            .iter()
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                Song::bytes_to_song(&value)
            })
            .collect()
    }

    pub fn clear(&self) {
        let keys: Vec<Vec<u8>> = self
            .queue_db
            .iter()
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
        for key in keys {
            _ = self.queue_db.remove(key);
        }
        _ = self.status_db.remove(CURRENT_SONG_KEY);
        _ = self.status_db.remove("priority_queue");
        self.reset_random_state();
    }

    pub fn query_current_queue(&self, query: CurrentQueueQuery) -> Option<PlaylistPage> {
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
                Some(PlaylistPage {
                    total,
                    offset: offset + page_size,
                    limit: page_size,
                    items: songs,
                })
            }
            CurrentQueueQuery::CurrentSongPage => {
                let songs = self.get_queue_page_starting_from_current_song(page_size);
                Some(PlaylistPage {
                    total: page_size,
                    offset: 0,
                    limit: page_size,
                    items: songs,
                })
            }
            CurrentQueueQuery::IgnoreSongs => None,
        }
    }

    pub fn set_current_to_last(&self) {
        if let Some(guard) = self.queue_db.last_key_value() {
            if let Ok(key) = guard.key() {
                _ = self.status_db.insert(CURRENT_SONG_KEY, key.as_ref());
            }
        }
    }

    pub fn add_songs_from_dir(&self, dir: &str) {
        self.song_repository.find_songs_by_dir_prefix(dir).for_each(|song| {
            self.add_song(&song);
        });
    }

    pub fn add_songs_after_current(&self, songs: impl IntoIterator<Item = Song>) -> Option<Vec<u8>> {
        let mut new_keys: Vec<Vec<u8>> = Vec::new();
        for song in songs {
            let id = self.generate_id();
            let key = id.to_be_bytes().to_vec();
            _ = self.queue_db.insert(&key, song.to_json_string_bytes());
            new_keys.push(key);
        }
        if new_keys.is_empty() {
            return None;
        }
        let mut pq = self.get_priority_queue();
        for key in new_keys.iter().rev() {
            if let Some(pos) = pq.iter().position(|k| k == key) {
                pq.remove(pos);
            }
            pq.insert(0, key.clone());
        }
        self.save_priority_queue(&pq);
        new_keys.into_iter().next()
    }

    pub fn add_songs_from_dir_after_current(&self, dir: &str) -> Option<Vec<u8>> {
        self.add_songs_after_current(self.song_repository.find_songs_by_dir_prefix(dir))
    }

    pub fn set_current_song(&self, key: &[u8]) {
        _ = self.status_db.insert(CURRENT_SONG_KEY, key);
    }

    pub fn add_song_after_current(&self, song_id: &str) {
        if let Some(song) = self.song_repository.find_by_id(song_id) {
            let id = self.generate_id();
            let key = id.to_be_bytes().to_vec();
            _ = self.queue_db.insert(&key, song.to_json_string_bytes());
            self.add_to_priority_queue(key);
        }
    }

    pub fn move_item_after_current(&self, from_index: usize) {
        let keys: Vec<Vec<u8>> = self
            .queue_db
            .iter()
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
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
        self.add_to_priority_queue(keys[target_index].clone());
    }

    pub fn move_item(&self, from_index: usize, to_index: usize) {
        let entries: Vec<(Vec<u8>, Vec<u8>)> = self
            .queue_db
            .iter()
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                Some((key.to_vec(), value.to_vec()))
            })
            .collect();
        let keys: Vec<Vec<u8>> = entries.iter().map(|(k, _)| k.clone()).collect();
        let mut values: Vec<Vec<u8>> = entries.into_iter().map(|(_, v)| v).collect();
        if from_index >= keys.len() || to_index >= keys.len() {
            return;
        }

        let current_key_opt = self.get_current_or_first_song_key();
        let current_index_opt = current_key_opt.and_then(|ck| keys.iter().position(|k| k == &ck));

        let val = values.remove(from_index);
        values.insert(to_index, val);

        let min_index = std::cmp::min(from_index, to_index);
        let max_index = std::cmp::max(from_index, to_index);
        for i in min_index..=max_index {
            _ = self.queue_db.insert(&keys[i], &values[i]);
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
        self.replace_all(self.song_repository.find_songs_by_dir_prefix(dir));
    }
}
