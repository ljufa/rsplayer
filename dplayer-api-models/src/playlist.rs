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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
    pub fn remove_item(&mut self, song_id: String) {
        self.items.retain(|s| s.id != song_id)
    }
}

impl Default for Playlists {
    fn default() -> Self {
        Self {
            items: Default::default(),
        }
    }
}
impl Playlists {
    pub fn has_saved(&self) -> bool {
        self.items.iter().any(|i| i.is_saved())
    }
    pub fn has_featured(&self) -> bool {
        self.items.iter().any(|i| i.is_featured())
    }
    pub fn has_new_releases(&self) -> bool {
        self.items.iter().any(|i| i.is_new_release())
    }
}
impl PlaylistType {
    pub fn is_saved(&self) -> bool {
        match *self {
            PlaylistType::Saved(_) => true,
            _ => false,
        }
    }
    pub fn is_featured(&self) -> bool {
        match *self {
            PlaylistType::Featured(_) => true,
            _ => false,
        }
    }

    pub fn is_new_release(&self) -> bool {
        match *self {
            PlaylistType::NewRelease(_) => true,
            _ => false,
        }
    }
}
impl Category {
    pub fn sanitized_name(&self) -> String {
        self.name.replace(
            &['(', ' ', '/', ')', '+', '&', ',', '\"', '.', ';', ':', '\''][..],
            "",
        )
    }
}
