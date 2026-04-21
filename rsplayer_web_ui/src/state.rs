use crate::vumeter::VisualizerType;
use api_models::{
    common::{MetadataLibraryItem, PlaybackMode, Volume},
    player::Song,
    playlist::{Album, PlaylistPage, Playlists},
    settings::Settings,
    stat::LibraryStats,
    state::{ExternalMount, MountStatus, MusicDirStatus, PlayerInfo, PlayerState, SongProgress, StateChangeEvent},
};
use dioxus::prelude::*;
use std::collections::HashMap;

/// Global application state shared across all components via context.
#[derive(Clone)]
pub struct AppState {
    pub volume: Signal<Volume>,
    pub player_info: Signal<Option<PlayerInfo>>,
    pub current_song: Signal<Option<Song>>,
    pub progress: Signal<SongProgress>,
    pub playback_mode: Signal<PlaybackMode>,
    pub player_state: Signal<PlayerState>,
    pub current_queue: Signal<Option<PlaylistPage>>,
    /// Raw items returned by the last Metadata query (files/artists tree).
    pub metadata_local_items: Signal<Vec<MetadataLibraryItem>>,
    pub favorite_radio_stations: Signal<Vec<String>>,
    pub playlists: Signal<Option<Playlists>>,
    pub library_stats: Signal<Option<LibraryStats>>,
    pub playlist_items: Signal<Vec<Song>>,
    pub metadata_scan_msg: Signal<Option<String>>,
    pub notification: Signal<Option<StateChangeEvent>>,
    pub current_theme: Signal<String>,
    pub global_settings: Signal<Option<Settings>>,
    pub connected: Signal<bool>,
    pub startup_error: Signal<Option<String>>,
    pub mount_statuses: Signal<Vec<MountStatus>>,
    pub music_dir_statuses: Signal<Vec<MusicDirStatus>>,
    pub external_mounts: Signal<Vec<ExternalMount>>,
    // VU meter
    pub vu_left: Signal<u8>,
    pub vu_right: Signal<u8>,
    pub vu_meter_enabled: Signal<bool>,
    pub visualizer_type: Signal<VisualizerType>,
    /// Resolved album art URL for the current song (local /artwork/ or Last.fm).
    pub album_image: Signal<Option<String>>,
    /// Whether to show the album art as a background image (persisted in localStorage).
    pub show_bg_image: Signal<bool>,
    /// Lazily fetched albums by genre name (keyed by genre string).
    pub lazy_genre_albums: Signal<HashMap<String, Vec<Album>>>,
    /// Lazily fetched albums by decade label (keyed by decade string like "1990s").
    pub lazy_decade_albums: Signal<HashMap<String, Vec<Album>>>,
}

impl AppState {
    pub fn new() -> Self {
        let visualizer_type = (|| {
            let storage = web_sys::window()?.local_storage().ok()??;
            let value = storage.get_item("rsplayer_visualizer").ok()??;
            VisualizerType::from_str(&value)
        })()
        .unwrap_or(VisualizerType::Lissajous);

        Self {
            volume: Signal::new(Volume::default()),
            player_info: Signal::new(None),
            current_song: Signal::new(None),
            progress: Signal::new(SongProgress::default()),
            playback_mode: Signal::new(PlaybackMode::default()),
            player_state: Signal::new(PlayerState::STOPPED),
            current_queue: Signal::new(None),
            metadata_local_items: Signal::new(Vec::new()),
            favorite_radio_stations: Signal::new(Vec::new()),
            playlists: Signal::new(None),
            library_stats: Signal::new(None),
            playlist_items: Signal::new(Vec::new()),
            metadata_scan_msg: Signal::new(None),
            notification: Signal::new(None),
            current_theme: Signal::new({
                (|| {
                    let storage = web_sys::window()?.local_storage().ok()??;
                    storage.get_item("rsplayer_theme").ok()?
                })()
                .unwrap_or_else(|| "dark".to_string())
            }),
            global_settings: Signal::new(None),
            connected: Signal::new(false),
            startup_error: Signal::new(None),
            mount_statuses: Signal::new(Vec::new()),
            music_dir_statuses: Signal::new(Vec::new()),
            external_mounts: Signal::new(Vec::new()),
            vu_left: Signal::new(0),
            vu_right: Signal::new(0),
            vu_meter_enabled: Signal::new(false),
            visualizer_type: Signal::new(visualizer_type),
            album_image: Signal::new(None),
            lazy_genre_albums: Signal::new(HashMap::new()),
            lazy_decade_albums: Signal::new(HashMap::new()),
            show_bg_image: Signal::new({
                (|| {
                    let storage = web_sys::window()?.local_storage().ok()??;
                    let v = storage.get_item("rsplayer_show_bg_image").ok()??;
                    Some(v != "false")
                })()
                .unwrap_or(true)
            }),
        }
    }

    /// Dispatch a state change event received from the WebSocket into signals.
    pub fn dispatch(&mut self, event: StateChangeEvent) {
        match &event {
            StateChangeEvent::VolumeChangeEvent(vol) => {
                *self.volume.write() = *vol;
            }
            StateChangeEvent::PlayerInfoEvent(pi) => {
                *self.player_info.write() = Some(pi.clone());
            }
            StateChangeEvent::CurrentSongEvent(song) => {
                // Resolve local album art synchronously; Last.fm is handled async in App.
                let local = song
                    .image_id
                    .as_ref()
                    .map(|id| format!("/artwork/{}", id))
                    .or_else(|| song.image_url.clone());
                *self.album_image.write() = local;
                *self.current_song.write() = Some(song.clone());
            }
            StateChangeEvent::SongTimeEvent(st) => {
                *self.progress.write() = st.clone();
                *self.player_state.write() = PlayerState::PLAYING;
            }
            StateChangeEvent::PlaybackModeChangedEvent(mode) => {
                *self.playback_mode.write() = *mode;
            }
            StateChangeEvent::PlaybackStateEvent(ps) => {
                *self.player_state.write() = ps.clone();
                if !matches!(ps, PlayerState::PLAYING) {
                    *self.vu_left.write() = 0;
                    *self.vu_right.write() = 0;
                }
            }
            StateChangeEvent::MetadataSongScanStarted => {
                *self.metadata_scan_msg.write() = Some("Music directory scanning started.".to_string());
            }
            StateChangeEvent::MetadataSongScanned(info) => {
                *self.metadata_scan_msg.write() = Some(info.clone());
            }
            StateChangeEvent::MetadataSongScanFinished(info) => {
                *self.metadata_scan_msg.write() = Some(info.clone());
            }
            StateChangeEvent::CurrentQueueEvent(page) => {
                *self.current_queue.write() = page.clone();
            }
            StateChangeEvent::MetadataLocalItems(items) => {
                *self.metadata_local_items.write() = items.clone();
            }
            StateChangeEvent::FavoriteRadioStations(stations) => {
                *self.favorite_radio_stations.write() = stations.clone();
            }
            StateChangeEvent::PlaylistsEvent(playlists) => {
                *self.playlists.write() = Some(playlists.clone());
            }
            StateChangeEvent::PlaylistItemsEvent(items, _page) => {
                *self.playlist_items.write() = items.clone();
            }
            StateChangeEvent::LibraryStatsEvent(stats) => {
                *self.library_stats.write() = Some(stats.clone());
            }
            StateChangeEvent::MountStatusEvent(statuses) => {
                *self.mount_statuses.write() = statuses.clone();
            }
            StateChangeEvent::MusicDirStatusEvent(statuses) => {
                *self.music_dir_statuses.write() = statuses.clone();
            }
            StateChangeEvent::ExternalMountsEvent(mounts) => {
                *self.external_mounts.write() = mounts.clone();
            }
            StateChangeEvent::GenreAlbumsEvent(genre, albums) => {
                self.lazy_genre_albums.write().insert(genre.clone(), albums.clone());
            }
            StateChangeEvent::DecadeAlbumsEvent(decade, albums) => {
                self.lazy_decade_albums.write().insert(decade.clone(), albums.clone());
            }
            StateChangeEvent::NotificationSuccess(_) | StateChangeEvent::NotificationError(_) => {
                *self.notification.write() = Some(event.clone());
            }
            StateChangeEvent::VUEvent(l, r) => {
                *self.vu_left.write() = *l;
                *self.vu_right.write() = *r;
            }
            StateChangeEvent::VuMeterEnabledEvent(enabled) => {
                *self.vu_meter_enabled.write() = *enabled;
            }
            _ => {}
        }
    }
}
