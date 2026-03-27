use fjall::{Database, Keyspace, KeyspaceCreateOptions};

use api_models::{
    player::Song,
    playlist::{Playlist, PlaylistPage, PlaylistType, Playlists},
};

pub struct PlaylistService {
    main_db: Keyspace,
    pl_tree: Keyspace,
}

impl PlaylistService {
    #[must_use]
    pub fn new(db: &Database) -> Self {
        let main_db = db
            .keyspace("playlist", KeyspaceCreateOptions::default)
            .expect("Failed to open playlist keyspace");
        let pl_tree = db
            .keyspace("playlist_list", KeyspaceCreateOptions::default)
            .expect("Failed to open playlist_list keyspace");
        Self { main_db, pl_tree }
    }

    pub fn save_new_playlist(&self, playlist_name: &str, songs: &[Song]) {
        if songs.is_empty() {
            return;
        }
        let keys_to_remove: Vec<Vec<u8>> = self
            .main_db
            .prefix(playlist_name)
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
        for key in keys_to_remove {
            _ = self.main_db.remove(key);
        }
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
        let entries: Vec<Vec<u8>> = self
            .main_db
            .prefix(playlist_name)
            .filter_map(|guard| guard.value().ok().map(|v| v.to_vec()))
            .collect();
        let total = entries.len();
        let songs: Vec<Song> = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|value| Song::bytes_to_song(&value).expect("Failed to deserialize song"))
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
                .filter_map(|guard| {
                    let value = guard.value().ok()?;
                    Some(PlaylistType::Saved(serde_json::from_slice(&value).ok()?))
                })
                .collect(),
        }
    }
}
