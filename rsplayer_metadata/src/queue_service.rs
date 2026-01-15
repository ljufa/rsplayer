use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc, RwLock,
};
use std::str::FromStr;

use rand::Rng;
use sled::{Db, IVec, Tree};

use api_models::{common::PlaybackMode, player::Song, playlist::PlaylistPage, settings::PlaybackQueueSetting, state::CurrentQueueQuery};

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

    #[allow(clippy::branches_sharing_code)]
    pub fn move_current_to_next_song(&self) -> bool {
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
        iter.for_each(|song| {
            let key = self.queue_db.generate_id().unwrap().to_be_bytes();
            _ = self.queue_db.insert(key, song.to_json_string_bytes());
        });
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

    pub fn add_songs_from_dir(&self, dir: &str) {
        self.song_repository
            .get_all_iterator()
            .filter(|item| item.file.starts_with(dir))
            .for_each(|song| {
                self.add_song(&song);
            });
    }
    pub fn load_songs_from_dir(&self, dir: &str) {
        self.replace_all(
            self.song_repository
                .get_all_iterator()
                .filter(|item| item.file.starts_with(dir)),
        );
    }
}
