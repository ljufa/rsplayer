use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::player::Song;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct Album {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released: Option<DateTime<Utc>>,
    pub added: DateTime<Utc>,
    pub song_keys: Vec<String>,
}

impl Album {
    pub fn from_bytes(value: &[u8]) -> Self {
        let album: Album = serde_json::from_slice(value).expect("Failed to deserialize album!");
        album
    }
    pub fn to_json_string_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Album serialization failed!")
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub name: String,
    pub id: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub owner_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PlaylistType {
    Saved(Playlist),
    Featured(Playlist),
    LatestRelease(Album),
    RecentlyAdded(Album),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct Playlists {
    pub items: Vec<PlaylistType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlaylistPage {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<Song>,
}

impl PlaylistPage {
    pub fn remove_item(&mut self, song_id: &str) {
        self.items.retain(|s| s.file != song_id);
    }
}

impl Playlists {
    pub fn has_saved(&self) -> bool {
        self.items.iter().any(PlaylistType::is_saved)
    }
    pub fn has_featured(&self) -> bool {
        self.items.iter().any(PlaylistType::is_featured)
    }
    pub fn has_new_releases(&self) -> bool {
        self.items.iter().any(PlaylistType::is_new_release)
    }
    pub fn has_recently_added(&self) -> bool {
        self.items.iter().any(PlaylistType::is_recently_added)
    }
}

impl PlaylistType {
    #[must_use]
    pub const fn is_saved(&self) -> bool {
        matches!(*self, Self::Saved(_))
    }
    #[must_use]
    pub const fn is_featured(&self) -> bool {
        matches!(*self, Self::Featured(_))
    }
    #[must_use]
    pub const fn is_new_release(&self) -> bool {
        matches!(*self, Self::LatestRelease(_))
    }
    #[must_use]
    pub const fn is_recently_added(&self) -> bool {
        matches!(*self, Self::RecentlyAdded(_))
    }
}
