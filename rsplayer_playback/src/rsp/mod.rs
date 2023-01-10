use std::{sync::Arc, thread::JoinHandle, time::Duration, vec};

use anyhow::Result;
use api_models::{
    player::Song,
    playlist::PlaylistPage,
    settings::{PlaybackQueueSetting, PlaylistSetting},
    state::{PlayerInfo, PlayerState, PlayingContext, PlayingContextQuery, SongProgress},
};

use mockall_double::double;

#[double]
use rsplayer_metadata::metadata::MetadataService;
use rsplayer_metadata::{playlist::PlaylistService, queue::PlaybackQueue};
use rspotify::sync::Mutex;

use crate::{
    get_dynamic_playlists, get_playlist_categories, Player, BY_ARTIST_PL_PREFIX, BY_DATE_PL_PREFIX,
    BY_FOLDER_PL_PREFIX, BY_GENRE_PL_PREFIX, SAVED_PL_PREFIX,
};

use self::symphonia::{PlaybackResult, SymphoniaPlayer};
mod output;
mod symphonia;
// #[cfg(test)]
// mod test;

const BY_FOLDER_DEPTH: usize = 6;
pub struct RsPlayer {
    queue: Arc<PlaybackQueue>,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
    symphonia_player: SymphoniaPlayer,
    play_handle: Arc<Mutex<Vec<JoinHandle<Result<PlaybackResult>>>>>,
}
impl RsPlayer {
    pub fn new(metadata_service: Arc<MetadataService>, audio_device: String) -> Self {
        let queue = Arc::new(PlaybackQueue::new(&PlaybackQueueSetting::default()));
        RsPlayer {
            queue: queue.clone(),
            metadata_service,
            playlist_service: Arc::new(PlaylistService::new(&PlaylistSetting::default())),
            symphonia_player: SymphoniaPlayer::new(queue, audio_device),
            play_handle: Arc::new(Mutex::new(vec![])),
        }
    }

    fn await_playing_song_to_finish(&self) -> Vec<Result<PlaybackResult>> {
        let mut results = vec![];
        self.play_handle.lock().unwrap().drain(..).for_each(|r| {
            if let Ok(res) = r.join() {
                results.push(res);
            }
        });
        results
    }
}

impl Player for RsPlayer {
    fn play_queue_from_current_song(&self) {
        self.play_handle
            .lock()
            .unwrap()
            .push(self.symphonia_player.play_all_in_queue());
    }

    fn pause_current_song(&self) {
        self.stop_current_song();
    }

    fn play_next_song(&self) {
        if self.queue.move_current_to_next_song() {
            self.stop_current_song();
            self.play_queue_from_current_song();
        }
    }

    fn play_prev_song(&self) {
        if self.queue.move_current_to_previous_song() {
            self.stop_current_song();
            self.play_queue_from_current_song();
        }
    }

    fn stop_current_song(&self) {
        self.symphonia_player.stop_playing();
        self.await_playing_song_to_finish();
    }

    fn seek_current_song(&self, _seconds: i8) {
        // todo!()
    }

    fn play_song(&self, song_id: &str) {
        if self.queue.move_current_to(song_id) {
            self.stop_current_song();
            self.play_queue_from_current_song();
        }
    }

    fn get_current_song(&self) -> Option<Song> {
        self.queue.get_current_song()
    }

    fn load_playlist_in_queue(&self, pl_id: &str) {
        self.stop_current_song();
        if pl_id.starts_with(BY_GENRE_PL_PREFIX) {
            let genre = pl_id.replace(BY_GENRE_PL_PREFIX, "");
            self.queue.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.genre == Some(genre.clone())),
            );
        } else if pl_id.starts_with(BY_DATE_PL_PREFIX) {
            let date = pl_id.replace(BY_DATE_PL_PREFIX, "");
            self.queue.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.date == Some(date.clone())),
            );
        } else if pl_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let artist = pl_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.queue.replace_all(
                self.metadata_service
                    .get_all_songs_iterator()
                    .filter(|s| s.artist == Some(artist.clone())),
            );
        } else if pl_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let folder = pl_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.queue
                .replace_all(self.metadata_service.get_all_songs_iterator().filter(|s| {
                    s.file
                        .split('/')
                        .nth(BY_FOLDER_DEPTH)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(folder.as_str())
                }));
        } else {
            let pl_songs = self
                .playlist_service
                .get_playlist_page_by_name(pl_id, 0, 20000)
                .items;
            self.queue.replace_all(pl_songs.into_iter());
        }
        self.play_queue_from_current_song();
    }

    fn load_album_in_queue(&self, _album_id: &str) {
        // todo!()
    }

    fn load_song_in_queue(&self, song_id: &str) {
        if let Some(song) = self.metadata_service.get_song(song_id).as_ref() {
            self.stop_current_song();
            self.queue.clear();
            self.queue.add_song(song);
            self.play_queue_from_current_song();
        }
    }

    fn remove_song_from_queue(&self, song_id: &str) {
        self.queue.remove_song(song_id);
    }

    fn add_song_in_queue(&self, song_id: &str) {
        if let Some(song) = self.metadata_service.get_song(song_id).as_ref() {
            self.queue.add_song(song);
        }
    }

    fn clear_queue(&self) {
        self.stop_current_song();
        self.queue.clear();
    }

    fn get_playlist_categories(&self) -> Vec<api_models::playlist::Category> {
        get_playlist_categories()
    }

    fn get_static_playlists(&self) -> api_models::playlist::Playlists {
        self.playlist_service.get_playlists()
    }

    fn get_dynamic_playlists(
        &self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<api_models::playlist::DynamicPlaylistsPage> {
        let all_songs: Vec<Song> = self.metadata_service.get_all_songs_iterator().collect();
        get_dynamic_playlists(category_ids, &all_songs, offset, limit, BY_FOLDER_DEPTH)
    }

    fn get_playlist_items(&self, playlist_id: &str) -> Vec<Song> {
        if playlist_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_GENRE_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.genre.as_ref().map_or(false, |g| *g == pl_name))
                .take(100)
                .collect()
        } else if playlist_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.artist.as_ref().map_or(false, |a| *a == pl_name))
                .take(100)
                .collect()
        } else if playlist_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_DATE_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.date.as_ref().map_or(false, |d| *d == pl_name))
                .take(100)
                .collect()
        } else if playlist_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| {
                    s.file
                        .split('/')
                        .nth(BY_FOLDER_DEPTH)
                        .map_or(false, |d| *d == pl_name)
                })
                .take(100)
                .collect()
        } else {
            let pl_name = playlist_id.replace(SAVED_PL_PREFIX, "");
            self.playlist_service
                .get_playlist_page_by_name(&pl_name, 0, 100)
                .items
        }
    }

    fn save_queue_as_playlist(&self, playlist_name: &str) {
        self.playlist_service
            .save_new_playlist(playlist_name, &self.queue.get_all_songs());
    }

    fn get_player_info(&self) -> Option<api_models::state::PlayerInfo> {
        let random_next = self.queue.get_random_next();
        if self.symphonia_player.is_playing() {
            Some(PlayerInfo {
                state: Some(PlayerState::PLAYING),
                random: Some(random_next),
                ..Default::default()
            })
        } else {
            Some(PlayerInfo {
                state: Some(PlayerState::PAUSED),
                random: Some(random_next),
                ..Default::default()
            })
        }
    }

    fn get_playing_context(
        &self,
        query: api_models::state::PlayingContextQuery,
    ) -> Option<api_models::state::PlayingContext> {
        let mut pc = PlayingContext {
            id: "1".to_string(),
            name: "Queue".to_string(),
            player_type: api_models::common::PlayerType::RSP,
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
                let (total, songs) = self.queue.get_queue_page(offset, page_size, |song| {
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
                let songs = self
                    .queue
                    .get_queue_page_starting_from_current_song(page_size);
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

    fn get_song_progress(&self) -> api_models::state::SongProgress {
        let time = self.symphonia_player.get_time();
        SongProgress {
            total_time: Duration::from_secs(time.0),
            current_time: Duration::from_secs(time.1),
        }
    }

    fn toggle_random_play(&self) {
        self.queue.toggle_random_next();
    }

    fn rescan_metadata(&self) {
        // todo!()
    }
}
