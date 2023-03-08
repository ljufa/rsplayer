use std::collections::HashSet;

use api_models::player::Song;
use api_models::playlist::{Category, DynamicPlaylistsPage, Playlist, Playlists};
use api_models::state::{PlayerInfo, PlayingContext, PlayingContextQuery, SongProgress};

pub mod mpd;
pub mod player_service;
pub mod rsp;
pub mod spotify;

pub trait Player {
    // Song
    fn play_from_current_queue_song(&self);
    fn pause_current_song(&self);
    fn play_next_song(&self);
    fn play_prev_song(&self);
    fn stop_current_song(&self);
    fn seek_current_song(&self, seconds: i8);
    fn play_song(&self, id: &str);

    // Queue
    fn get_current_song(&self) -> Option<Song>;
    fn load_playlist_in_queue(&self, pl_id: &str);
    fn load_album_in_queue(&self, album_id: &str);
    fn load_song_in_queue(&self, song_id: &str);
    fn remove_song_from_queue(&self, id: &str);
    fn add_song_in_queue(&self, song_id: &str);
    fn clear_queue(&self);

    // Playlist
    fn get_playlist_categories(&self) -> Vec<Category>;
    fn get_static_playlists(&self) -> Playlists;
    fn get_dynamic_playlists(
        &self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage>;
    fn get_playlist_items(&self, playlist_id: &str, page_no: usize) -> Vec<Song>;
    fn save_queue_as_playlist(&self, playlist_name: &str);

    // Player
    fn get_player_info(&self) -> Option<PlayerInfo>;
    fn get_playing_context(&self, query: PlayingContextQuery) -> Option<PlayingContext>;
    fn get_song_progress(&self) -> SongProgress;
    fn toggle_random_play(&self);

    // Metadata????
    fn rescan_metadata(&self);
}

const BY_GENRE_PL_PREFIX: &str = "playlist_by_genre_";
const BY_DATE_PL_PREFIX: &str = "playlist_by_date_";
const BY_ARTIST_PL_PREFIX: &str = "playlist_by_artist_";
const BY_FOLDER_PL_PREFIX: &str = "playlist_by_folder_";
const SAVED_PL_PREFIX: &str = "playlist_saved_";

const CATEGORY_ID_BY_GENRE: &str = "category_by_genre";
const CATEGORY_ID_BY_DATE: &str = "category_by_date";
const CATEGORY_ID_BY_ARTIST: &str = "category_by_artist";
const CATEGORY_ID_BY_FOLDER: &str = "category_by_folder";

fn get_playlist_categories() -> Vec<Category> {
    vec![
        Category {
            id: CATEGORY_ID_BY_ARTIST.to_string(),
            name: "By Artist".to_string(),
            icon: String::new(),
        },
        Category {
            id: CATEGORY_ID_BY_DATE.to_string(),
            name: "By Date".to_string(),
            icon: String::new(),
        },
        Category {
            id: CATEGORY_ID_BY_GENRE.to_string(),
            name: "By Genre".to_string(),
            icon: String::new(),
        },
        Category {
            id: CATEGORY_ID_BY_FOLDER.to_string(),
            name: "By Directory".to_string(),
            icon: String::new(),
        },
    ]
}

fn get_dynamic_playlists(
    category_ids: Vec<String>,
    all_songs: &[Song],
    offset: u32,
    limit: u32,
    by_folder_depth: usize,
) -> Vec<DynamicPlaylistsPage> {
    let mut result = vec![];
    for category_id in category_ids {
        result.push(match category_id.as_str() {
            CATEGORY_ID_BY_ARTIST => DynamicPlaylistsPage {
                category_id: CATEGORY_ID_BY_ARTIST.to_string(),
                playlists: get_playlists_by_artist(all_songs, offset, limit),
                offset,
                limit,
            },
            CATEGORY_ID_BY_DATE => DynamicPlaylistsPage {
                category_id: CATEGORY_ID_BY_DATE.to_string(),
                playlists: get_playlists_by_date(all_songs, offset, limit),
                offset,
                limit,
            },
            CATEGORY_ID_BY_GENRE => DynamicPlaylistsPage {
                category_id: CATEGORY_ID_BY_GENRE.to_string(),
                playlists: get_playlists_by_genre(all_songs, offset, limit),
                offset,
                limit,
            },
            CATEGORY_ID_BY_FOLDER => DynamicPlaylistsPage {
                category_id: CATEGORY_ID_BY_FOLDER.to_string(),
                playlists: get_playlists_by_folder(all_songs, offset, limit, by_folder_depth),
                offset,
                limit,
            },
            &_ => {
                todo!()
            }
        });
    }
    result
}

fn get_playlists_by_genre(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    let mut items = vec![];
    let mut genres: Vec<String> = all_songs
        .iter()
        .filter_map(|s| s.genre.clone())
        .filter(|g| g.starts_with(char::is_alphabetic))
        .collect();
    genres.sort();
    genres.dedup();
    genres
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|g| {
            items.push(Playlist {
                name: g.clone(),
                id: format!("{BY_GENRE_PL_PREFIX}{g}"),
                description: Some("Songs by genre ".to_string() + g),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_date(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    // dynamic pls
    let mut items = vec![];
    let mut dates: Vec<String> = all_songs.iter().filter_map(|s| s.date.clone()).collect();
    dates.sort();
    dates.dedup();
    dates.reverse();
    dates
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|date| {
            items.push(Playlist {
                name: date.clone(),
                id: format!("{BY_DATE_PL_PREFIX}{date}"),
                description: Some("Songs by date ".to_string() + date),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_artist(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    let mut items = vec![];
    let mut artists: Vec<String> = all_songs
        .iter()
        .filter_map(|s| s.artist.clone())
        .filter(|art| art.starts_with(char::is_alphabetic))
        .collect();
    artists.sort();
    artists.dedup();
    artists
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|art| {
            items.push(Playlist {
                name: art.clone(),
                id: format!("{BY_ARTIST_PL_PREFIX}{art}"),
                description: Some("Songs by artist ".to_string() + art),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_folder(
    all_songs: &[Song],
    offset: u32,
    limit: u32,
    depth: usize,
) -> Vec<Playlist> {
    let mut items = vec![];
    let second_level_folders: HashSet<String> = all_songs
        .iter()
        .map(|s| s.file.clone())
        .map(|file| file.split('/').nth(depth).unwrap_or_default().to_string())
        .collect();
    second_level_folders
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|folder| {
            items.push(Playlist {
                name: folder.clone(),
                id: format!("{BY_FOLDER_PL_PREFIX}{folder}"),
                description: Some("Songs by dir ".to_string() + folder),
                image: None,
                owner_name: None,
            });
        });
    items
}
