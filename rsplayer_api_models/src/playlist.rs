use serde::{Deserialize, Serialize};

use crate::player::Song;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default, PartialOrd, Ord)]
pub struct Category {
    pub id: String,
    pub icon: String,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct Album {
    pub id: String,
    pub album_name: String,
    pub album_type: String,
    pub images: Vec<String>,
    pub artists: Vec<String>,
    pub genres: Vec<String>,
    pub release_date: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
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
    NewRelease(Album),
}
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct DynamicPlaylistsPage {
    pub category_id: String,
    pub playlists: Vec<Playlist>,
    pub offset: u32,
    pub limit: u32,
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
        self.items.retain(|s| s.id != song_id);
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
}
impl PlaylistType {
    pub const fn is_saved(&self) -> bool {
        matches!(*self, PlaylistType::Saved(_))
    }
    pub const fn is_featured(&self) -> bool {
        matches!(*self, PlaylistType::Featured(_))
    }
    
    pub const fn is_new_release(&self) -> bool {
        matches!(*self, PlaylistType::NewRelease(_))
    }
}
impl Category {
    pub fn sanitized_id(&self) -> String {
        self.id.replace(
            &['(', ' ', '/', ')', '+', '&', ',', '\"', '.', ';', ':', '\''][..],
            "",
        )
    }
}
