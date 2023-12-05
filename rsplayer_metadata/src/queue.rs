use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use api_models::{
    common::{BY_ARTIST_PL_PREFIX, BY_DATE_PL_PREFIX, BY_FOLDER_PL_PREFIX, BY_GENRE_PL_PREFIX},
    player::Song,
    playlist::PlaylistPage,
    settings::PlaybackQueueSetting,
    state::{PlayingContext, PlayingContextQuery},
};
use rand::Rng;
use sled::{Db, IVec, Tree};

use crate::{metadata::MetadataService, playlist::PlaylistService};

pub struct QueueService {
    queue_db: Db,
    status_db: Tree,
    random_flag: AtomicBool,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
}
const CURRENT_SONG_KEY: &str = "current_song_key";
impl QueueService {
    #[must_use]
    pub fn new(
        settings: &PlaybackQueueSetting,
        metadata_service: Arc<MetadataService>,
        playlist_service: Arc<PlaylistService>,
    ) -> Self {
        let db = sled::open(&settings.db_path).expect("Failed to open queue db");
        let status_db = db.open_tree("status").expect("Failed to open status tree");
        let random_flag = status_db.contains_key("random_next").unwrap_or(false);
        Self {
            queue_db: db,
            status_db,
            random_flag: AtomicBool::new(random_flag),
            metadata_service,
            playlist_service,
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
        let queue_len = self.queue_db.len();
        if queue_len < 2 {
            return false;
        }
        if self.get_random_next() {
            let mut rnd = rand::thread_rng();
            let rand_position = rnd.gen_range(0..queue_len - 1);
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
        let Ok(Some(prev_entry)) = self.queue_db.get_lt(current_key) else {
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
    pub fn add_song_by_id(&self, song_id: &str) {
        self.metadata_service
            .find_song_by_id(song_id)
            .as_ref()
            .map_or_else(
                || {
                    if song_id.starts_with("http") {
                        self.add_song(&Song {
                            id: song_id.to_string(),
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
    pub fn get_current_playing_context(
        &self,
        query: PlayingContextQuery,
    ) -> Option<PlayingContext> {
        let mut pc = PlayingContext {
            id: "1".to_string(),
            name: "Queue".to_string(),
            context_type: api_models::state::PlayingContextType::Playlist {
                description: None,
                public: None,
                snapshot_id: "1".to_string(),
            },
            playlist_page: None,
            image_url: None,
        };
        let page_size = 100;
        match query {
            PlayingContextQuery::WithSearchTerm(term, offset) => {
                let (total, songs) = self.get_queue_page(offset, page_size, |song| {
                    if term.len() > 2 {
                        song.all_text()
                            .to_lowercase()
                            .contains(&term.to_lowercase())
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
                pc.playlist_page = Some(page);
            }
            PlayingContextQuery::CurrentSongPage => {
                let songs = self.get_queue_page_starting_from_current_song(page_size);
                let page = PlaylistPage {
                    total: page_size,
                    offset: 0,
                    limit: page_size,
                    items: songs,
                };
                pc.playlist_page = Some(page);
            }

            PlayingContextQuery::IgnoreSongs => {}
        }
        Some(pc)
    }
    pub fn load_playlist_in_queue(&self, pl_id: &str) {
        if pl_id.starts_with(BY_GENRE_PL_PREFIX) {
            let genre = pl_id.replace(BY_GENRE_PL_PREFIX, "");
            self.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.genre == Some(genre.clone())),
            );
        } else if pl_id.starts_with(BY_DATE_PL_PREFIX) {
            let date = pl_id.replace(BY_DATE_PL_PREFIX, "");
            self.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.date == Some(date.clone())),
            );
        } else if pl_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let artist = pl_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.artist == Some(artist.clone())),
            );
        } else if pl_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let folder = pl_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.replace_all(self.metadata_service.get_all_songs_iterator().filter(|s| {
                s.file
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .eq_ignore_ascii_case(folder.as_str())
            }));
        } else {
            let pl_songs = self
                .playlist_service
                .get_playlist_page_by_name(pl_id, 0, 20000)
                .items;
            self.replace_all(pl_songs.into_iter());
        }
    }

    pub fn add_songs_from_dir(&self, dir: &str) {
        self.metadata_service
            .get_all_songs_iterator()
            .filter(|item| item.file.starts_with(dir))
            .for_each(|song| {
                self.add_song(&song);
            });
    }
    pub fn load_songs_from_dir(&self, dir: &str) {
        self.replace_all(
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|item| item.file.starts_with(dir)),
        );
    }
}
