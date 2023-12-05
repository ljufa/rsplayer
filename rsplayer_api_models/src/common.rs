use std::time::Duration;

use crate::{player::Song, state::PlayingContextQuery};
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};

pub const BY_GENRE_PL_PREFIX: &str = "playlist_by_genre_";
pub const BY_DATE_PL_PREFIX: &str = "playlist_by_date_";
pub const BY_ARTIST_PL_PREFIX: &str = "playlist_by_artist_";
pub const BY_FOLDER_PL_PREFIX: &str = "playlist_by_folder_";
pub const SAVED_PL_PREFIX: &str = "playlist_saved_";

pub const CATEGORY_ID_BY_GENRE: &str = "category_by_genre";
pub const CATEGORY_ID_BY_DATE: &str = "category_by_date";
pub const CATEGORY_ID_BY_ARTIST: &str = "category_by_artist";
pub const CATEGORY_ID_BY_FOLDER: &str = "category_by_folder";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum MetadataLibraryItem {
    SongItem(Song),
    Directory { name: String },
    Empty,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataLibraryResult {
    pub items: Vec<MetadataLibraryItem>,
    pub root_path: String,
}

impl MetadataLibraryItem {
    pub fn get_title(&self) -> String {
        match self {
            MetadataLibraryItem::SongItem(song) => song.get_title(),
            MetadataLibraryItem::Directory { name } => name.to_string(),
            MetadataLibraryItem::Empty => String::new(),
        }
    }
    pub fn get_id(&self) -> String {
        match self {
            MetadataLibraryItem::Directory { name } => name.to_string(),
            MetadataLibraryItem::SongItem(song) => song.id.to_string(),
            MetadataLibraryItem::Empty => String::new(),
        }
    }
    pub const fn is_dir(&self) -> bool {
        matches!(self, MetadataLibraryItem::Directory { name: _ })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PcmOutputDevice {
    pub name: String,
    pub description: String,
    pub card_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CardMixer {
    pub index: u32,
    pub name: String,
    pub card_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioCard {
    pub index: i32,
    pub name: String,
    pub description: String,
    pub pcm_devices: Vec<PcmOutputDevice>,
    pub mixers: Vec<CardMixer>,
}

#[derive(
    Debug,
    Hash,
    Serialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    FromPrimitive,
    ToPrimitive,
    Deserialize,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
pub enum VolumeCrtlType {
    Dac,
    Alsa,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Volume {
    pub step: i64,
    pub min: i64,
    pub max: i64,
    pub current: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum UserCommand {
    Player(PlayerCommand),
    Queue(QueueCommand),
    Playlist(PlaylistCommand),
    Metadata(MetadataCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum PlayerCommand {
    // Player commands
    Next,
    Prev,
    Pause,
    Play,
    PlayItem(String),
    RandomToggle,
    Rewind(i8),
    QueryCurrentPlayerInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MetadataCommand {
    QueryLocalFiles(String, u32),
    RescanMetadata(String, bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum PlaylistCommand {
    QueryDynamicPlaylists(Vec<String>, u32, u32),
    SaveQueueAsPlaylist(String),
    QueryPlaylistItems(String, usize),
    QuerySavedPlaylist,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum QueueCommand {
    LoadPlaylistInQueue(String),
    LoadAlbumInQueue(String),
    LoadSongToQueue(String),
    AddSongToQueue(String),
    AddLocalLibDirectory(String),
    LoadLocalLibDirectory(String),
    ClearQueue,
    QueryCurrentSong,
    QueryCurrentPlayingContext(PlayingContextQuery),
    RemoveItem(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum SystemCommand {
    // System commands
    VolUp,
    VolDown,
    SetVol(u8),
    PowerOff,
    RestartSystem,
    RestartRSPlayer,
    ChangeAudioOutput,
    QueryCurrentStreamerState,
}

#[derive(
    Debug,
    Hash,
    Serialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    FromPrimitive,
    ToPrimitive,
    Deserialize,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
pub enum FilterType {
    SharpRollOff,
    SlowRollOff,
    ShortDelaySharpRollOff,
    ShortDelaySlowRollOff,
    SuperSlow,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, EnumIter, IntoStaticStr,
)]
pub enum GainLevel {
    V25,
    V28,
    V375,
}

pub enum MetadataProvider {
    LocalFiles,
    RadioBrowser,
}
pub struct ProtocolHandler {
    _prefix: String,
    _metadata_type: MetadataProvider,
}


#[must_use]
pub fn dur_to_string(duration: &Duration) -> String {
    let mut result = "00:00:00".to_string();
    let secs = duration.as_secs();
    if secs > 0 {
        let seconds = secs % 60;
        let minutes = (secs / 60) % 60;
        let hours = (secs / 60) / 60;
        result = format!("{hours:0>2}:{minutes:0>2}:{seconds:0>2}");
    }
    result
}

#[must_use]
pub fn to_database_key(input: &str) -> String {
    input.to_string()
}
