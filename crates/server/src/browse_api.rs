//! Read-only HTTP browse API: `GET /api/browse/...`.
//!
//! The WebSocket answers library queries by broadcasting an event to every
//! client, with no request id, which is fine for the one web UI but not for
//! a client that asks several things at once and must match each answer to
//! its question. The Android media browser (Android Auto) is such a client,
//! so the same data is offered here as plain request/response JSON, in the
//! `api_models` types. Commands (play, queue changes) stay on the WebSocket.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use api_models::player::Song;
use api_models::playlist::{Album, Playlist, PlaylistType};
use api_models::podcast::{Episode, Podcast};
use api_models::radio::RadioStation;
use metadata::metadata_service::MetadataService;
use metadata::playlist_service::PlaylistService;
use metadata::ports::{album_repository::ArcAlbumRepository, song_repository::ArcSongRepository};
use metadata::queue_service::QueueService;
use podcast::PodcastService;

/// Ids of the two playlists computed from play statistics.
pub const MOST_PLAYED_ID: &str = "most_played";
pub const LIKED_ID: &str = "liked";

/// Upper bounds, a car screen shows far fewer.
const DYNAMIC_PLAYLIST_SONGS: usize = 100;
const PLAYLIST_SONGS: usize = 500;
const RECENT_ALBUMS: usize = 50;
const EPISODES: usize = 100;
const QUEUE_SONGS: usize = 100;
const SEARCH_ARTISTS: usize = 10;
const SEARCH_ALBUMS: usize = 20;
const SEARCH_SONGS: usize = 50;

#[derive(Clone)]
pub struct BrowseState {
    pub album_repository: ArcAlbumRepository,
    pub song_repository: ArcSongRepository,
    pub metadata_service: Arc<MetadataService>,
    pub playlist_service: Arc<PlaylistService>,
    pub podcast_service: Arc<PodcastService>,
    pub queue_service: Arc<QueueService>,
}

pub fn router(state: BrowseState) -> Router {
    Router::new()
        .route("/api/browse/artists", get(artists))
        .route("/api/browse/albums", get(albums))
        .route("/api/browse/songs", get(songs))
        .route("/api/browse/playlists", get(playlists))
        .route("/api/browse/playlist_songs", get(playlist_songs))
        .route("/api/browse/radio", get(radio))
        .route("/api/browse/podcasts", get(podcasts))
        .route("/api/browse/episodes", get(episodes))
        .route("/api/browse/queue", get(queue))
        .route("/api/browse/search", get(search))
        .with_state(state)
}

#[derive(Deserialize)]
struct AlbumsQuery {
    /// Album artist. Without it the newest albums are returned, see `sort`.
    artist: Option<String>,
    /// For the newest albums: `released` orders by release date, anything
    /// else by the date the album was added to the library.
    sort: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Serialize)]
struct SearchResult {
    artists: Vec<String>,
    albums: Vec<Album>,
    songs: Vec<Song>,
}

#[derive(Deserialize)]
struct IdQuery {
    id: String,
}

/// All album artists.
async fn artists(State(state): State<BrowseState>) -> Json<Vec<String>> {
    Json(state.album_repository.find_all_album_artists())
}

async fn albums(State(state): State<BrowseState>, Query(query): Query<AlbumsQuery>) -> Json<Vec<Album>> {
    Json(match query.artist {
        Some(artist) => state.album_repository.find_by_artist(&artist),
        None if query.sort.as_deref() == Some("released") => {
            state.album_repository.find_all_sort_by_released_desc(RECENT_ALBUMS)
        }
        None => state.album_repository.find_all_sort_by_added_desc(RECENT_ALBUMS),
    })
}

/// Songs of the album with this id, in album order.
async fn songs(State(state): State<BrowseState>, Query(query): Query<IdQuery>) -> Json<Vec<Song>> {
    Json(
        state
            .album_repository
            .find_by_id(&query.id)
            .map(|album| {
                album
                    .song_keys
                    .iter()
                    .filter_map(|key| state.song_repository.find_by_id(key))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// Saved playlists, followed by "Most Played" and "Liked" when they have songs.
async fn playlists(State(state): State<BrowseState>) -> Json<Vec<Playlist>> {
    let mut result: Vec<Playlist> = state
        .playlist_service
        .get_playlists()
        .items
        .into_iter()
        .filter_map(|item| match item {
            PlaylistType::Saved(playlist) => Some(playlist),
            _ => None,
        })
        .collect();
    if let Some(first) = state.metadata_service.get_most_played_songs(1).first() {
        result.push(Playlist {
            id: MOST_PLAYED_ID.to_string(),
            name: "Most Played".to_string(),
            description: Some("Your most played tracks".to_string()),
            image: first.image_id.clone(),
            owner_name: None,
        });
    }
    if let Some(first) = state.metadata_service.get_liked_songs(1).first() {
        result.push(Playlist {
            id: LIKED_ID.to_string(),
            name: "Liked".to_string(),
            description: Some("Songs you liked".to_string()),
            image: first.image_id.clone(),
            owner_name: None,
        });
    }
    Json(result)
}

async fn playlist_songs(State(state): State<BrowseState>, Query(query): Query<IdQuery>) -> Json<Vec<Song>> {
    Json(match query.id.as_str() {
        MOST_PLAYED_ID => state.metadata_service.get_most_played_songs(DYNAMIC_PLAYLIST_SONGS),
        LIKED_ID => state.metadata_service.get_liked_songs(DYNAMIC_PLAYLIST_SONGS),
        id => state.playlist_service.get_playlist_page_by_name(id, 0, PLAYLIST_SONGS).items,
    })
}

/// Hand-added stations followed by the liked radio-browser stations.
async fn radio(State(state): State<BrowseState>) -> Json<Vec<RadioStation>> {
    let mut stations = state.metadata_service.get_custom_radio_stations();
    stations.extend(state.metadata_service.get_favorite_radio_station_records());
    Json(stations)
}

/// Subscribed podcasts.
async fn podcasts(State(state): State<BrowseState>) -> Json<Vec<Podcast>> {
    Json(state.podcast_service.list_podcasts())
}

/// The newest episodes of the podcast with this id.
async fn episodes(State(state): State<BrowseState>, Query(query): Query<IdQuery>) -> Json<Vec<Episode>> {
    Json(state.podcast_service.episodes_page(&query.id, 0, EPISODES).episodes)
}

/// The queue from the current song onwards.
async fn queue(State(state): State<BrowseState>) -> Json<Vec<Song>> {
    Json(state.queue_service.get_queue_page_starting_from_current_song(QUEUE_SONGS))
}

/// Case-insensitive substring search: album artists and album titles by
/// name, songs by title, artist or album.
async fn search(State(state): State<BrowseState>, Query(query): Query<SearchQuery>) -> Json<SearchResult> {
    let term = query.q.trim().to_lowercase();
    if term.is_empty() {
        return Json(SearchResult {
            artists: Vec::new(),
            albums: Vec::new(),
            songs: Vec::new(),
        });
    }
    // The library is scanned in full, off the async workers.
    let result = tokio::task::spawn_blocking(move || {
        let matches = |text: &Option<String>| text.as_ref().is_some_and(|t| t.to_lowercase().contains(&term));
        SearchResult {
            artists: state
                .album_repository
                .find_all_album_artists()
                .into_iter()
                .filter(|artist| artist.to_lowercase().contains(&term))
                .take(SEARCH_ARTISTS)
                .collect(),
            albums: state
                .album_repository
                .find_all()
                .into_iter()
                .filter(|album| album.title.to_lowercase().contains(&term))
                .take(SEARCH_ALBUMS)
                .collect(),
            songs: state
                .song_repository
                .find_all()
                .into_iter()
                .filter(|song| matches(&song.title) || matches(&song.artist) || matches(&song.album))
                .take(SEARCH_SONGS)
                .collect(),
        }
    })
    .await;
    Json(result.unwrap_or_else(|e| {
        log::error!("Library search failed: {e}");
        SearchResult {
            artists: Vec::new(),
            albums: Vec::new(),
            songs: Vec::new(),
        }
    }))
}
