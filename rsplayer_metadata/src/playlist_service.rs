use sled::{Db, Tree};

use api_models::{
    player::Song,
    playlist::{Playlist, PlaylistPage, PlaylistType, Playlists},
    settings::PlaylistSetting,
};

pub struct PlaylistService {
    main_db: Db,
    pl_tree: Tree,
}

impl PlaylistService {
    #[must_use]
    pub fn new(settings: &PlaylistSetting) -> Self {
        let song_pl_db = sled::open(&settings.db_path).expect("Failed to open playlist database");
        let pl_tree = song_pl_db.open_tree("pl_tree").expect("Failed to open pl_list_tree");
        Self {
            main_db: song_pl_db,
            pl_tree,
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
            _ = self
                .main_db
                .insert(format!("{playlist_name}_{idx}").as_str(), song.to_json_string_bytes());
        }
        let pl = Playlist {
            name: playlist_name.to_string(),
            id: playlist_name.to_string(),
            description: None,
            image: None,
            owner_name: None,
        };
        _ = self
            .pl_tree
            .insert(playlist_name, serde_json::to_vec(&pl).expect("failed to serialize"));
    }

    pub fn get_playlist_page_by_name(&self, playlist_name: &str, offset: usize, limit: usize) -> PlaylistPage {
        let total = self.main_db.scan_prefix(playlist_name).filter_map(Result::ok).count();
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
}
