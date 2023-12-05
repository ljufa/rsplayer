use std::{collections::HashSet, sync::Arc};

use api_models::{
    common::{
        CATEGORY_ID_BY_ARTIST, CATEGORY_ID_BY_DATE, CATEGORY_ID_BY_FOLDER, CATEGORY_ID_BY_GENRE,
        SAVED_PL_PREFIX,
    },
    player::Song,
    playlist::{Category, DynamicPlaylistsPage, Playlist, PlaylistPage, PlaylistType, Playlists},
    settings::PlaylistSetting,
};
use sled::{Db, Tree};

use crate::metadata::MetadataService;
use api_models::common::{
    BY_ARTIST_PL_PREFIX, BY_DATE_PL_PREFIX, BY_FOLDER_PL_PREFIX, BY_GENRE_PL_PREFIX,
};

pub struct PlaylistService {
    main_db: Db,
    pl_tree: Tree,
    metadata_service: Arc<MetadataService>,
}
impl PlaylistService {
    #[must_use]
    pub fn new(settings: &PlaylistSetting, metadata_service: Arc<MetadataService>) -> Self {
        let song_pl_db = sled::open(&settings.db_path).expect("Failed to open playlist database");
        let pl_tree = song_pl_db
            .open_tree("pl_tree")
            .expect("Failed to open pl_list_tree");
        Self {
            main_db: song_pl_db,
            pl_tree,
            metadata_service,
        }
    }

    pub fn save_new_playlist(&self, playlist_name: &str, songs: &[Song]) {
        if songs.is_empty() {
            return;
        }
        self.main_db
            .scan_prefix(playlist_name)
            .filter_map(Result::ok)
            .for_each(|ex| {
                _ = self.main_db.remove(ex.0);
            });
        for (idx, song) in songs.iter().enumerate() {
            _ = self.main_db.insert(
                format!("{playlist_name}_{idx}").as_str(),
                song.to_json_string_bytes(),
            );
        }
        let pl = Playlist {
            name: playlist_name.to_string(),
            id: playlist_name.to_string(),
            description: None,
            image: None,
            owner_name: None,
        };
        _ = self.pl_tree.insert(
            playlist_name,
            serde_json::to_vec(&pl).expect("failed to serialize"),
        );
    }

    pub fn get_playlist_page_by_name(
        &self,
        playlist_name: &str,
        offset: usize,
        limit: usize,
    ) -> PlaylistPage {
        let total = self
            .main_db
            .scan_prefix(playlist_name)
            .filter_map(Result::ok)
            .count();
        let songs: Vec<Song> = self
            .main_db
            .scan_prefix(playlist_name)
            .filter_map(Result::ok)
            .skip(offset)
            .take(limit)
            .map(|entry| Song::bytes_to_song(&entry.1).expect("Failed to deserialize song"))
            .collect();
        PlaylistPage {
            total,
            offset,
            limit,
            items: songs,
        }
    }

    pub fn get_playlists(&self) -> Playlists {
        Playlists {
            items: self
                .pl_tree
                .iter()
                .filter_map(Result::ok)
                .map(|ple| PlaylistType::Saved(serde_json::from_slice(&ple.1).ok().unwrap()))
                .collect(),
        }
    }

    pub fn get_dynamic_playlists(
        &self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<api_models::playlist::DynamicPlaylistsPage> {
        let all_songs: Vec<Song> = self.metadata_service.get_all_songs_iterator().collect();
        get_dynamic_playlists(category_ids, &all_songs, offset, limit, 0)
    }

    pub fn get_dynamic_playlist_items(&self, playlist_id: &str, page_no: usize) -> Vec<Song> {
        let items_page_size: usize = 100;
        let offset: usize = if page_no > 1 {
            page_no * items_page_size
        } else {
            0
        };
        if playlist_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_GENRE_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.genre.as_ref().map_or(false, |g| *g == pl_name))
                .skip(offset)
                .take(items_page_size)
                .collect()
        } else if playlist_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.artist.as_ref().map_or(false, |a| *a == pl_name))
                .skip(offset)
                .take(items_page_size)
                .collect()
        } else if playlist_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_DATE_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.date.as_ref().map_or(false, |d| *d == pl_name))
                .skip(offset)
                .take(items_page_size)
                .collect()
        } else if playlist_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.metadata_service
                .get_all_songs_iterator()
                .filter(|s| s.file.split('/').next().map_or(false, |d| *d == pl_name))
                .skip(offset)
                .take(items_page_size)
                .collect()
        } else {
            let pl_name = playlist_id.replace(SAVED_PL_PREFIX, "");
            self.get_playlist_page_by_name(&pl_name, offset, items_page_size)
                .items
        }
    }
    pub fn get_playlist_categories() -> Vec<Category> {
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
