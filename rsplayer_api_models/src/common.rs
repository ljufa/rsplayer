use std::time::Duration;

use crate::{player::Song, state::CurrentQueueQuery};
use chrono::{DateTime, Utc};
use num_derive::ToPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum MetadataLibraryItem {
    SongItem(Song),
    Directory { name: String },
    Artist { name: String },
    Album { name: String, year: Option<DateTime<Utc>> },
    Empty,
}

impl MetadataLibraryItem {
    pub fn get_title(&self) -> String {
        match self {
            MetadataLibraryItem::SongItem(song) => song.get_title(),
            MetadataLibraryItem::Directory { name } | MetadataLibraryItem::Artist { name } => name.to_string(),
            MetadataLibraryItem::Album { name, year } => year
                .as_ref()
                .map_or_else(|| name.to_string(), |year| format!("{name} ({year})")),
            MetadataLibraryItem::Empty => String::new(),
        }
    }
    pub fn get_id(&self) -> String {
        match self {
            MetadataLibraryItem::Directory { name } => format!("{name}/"),
            MetadataLibraryItem::Artist { name } | MetadataLibraryItem::Album { name, year: _ } => name.to_owned(),
            MetadataLibraryItem::SongItem(song) => song.get_file_name_without_path(),
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
    Debug, Hash, Serialize, Clone, Copy, PartialEq, Eq, ToPrimitive, Deserialize, EnumString, EnumIter, IntoStaticStr,
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
    QueryLocalFiles(String, usize),
    SearchLocalFiles(String, usize),
    QueryArtists,
    SearchArtists(String),
    QueryAlbumsByArtist(String),
    QuerySongsByAlbum(String),
    RescanMetadata(String, bool),
    LikeMediaItem(String),
    DislikeMediaItem(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum PlaylistCommand {
    SaveQueueAsPlaylist(String),
    QueryPlaylistItems(String, usize),
    QueryAlbumItems(String, usize),
    QueryPlaylist,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum QueueCommand {
    LoadPlaylistInQueue(String),
    LoadAlbumInQueue(String),
    LoadArtistInQueue(String),
    LoadSongToQueue(String),
    LoadLocalLibDirectory(String),
    AddSongToQueue(String),
    AddArtistToQueue(String),
    AddLocalLibDirectory(String),
    AddPlaylistToQueue(String),
    AddAlbumToQueue(String),
    ClearQueue,
    QueryCurrentSong,
    QueryCurrentQueue(CurrentQueueQuery),
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
    Debug, Hash, Serialize, Clone, Copy, PartialEq, Eq, ToPrimitive, Deserialize, EnumString, EnumIter, IntoStaticStr,
)]
pub enum FilterType {
    SharpRollOff,
    SlowRollOff,
    ShortDelaySharpRollOff,
    ShortDelaySlowRollOff,
    SuperSlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, EnumIter, IntoStaticStr)]
pub enum GainLevel {
    V25,
    V28,
    V375,
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
