use std::{fmt::Display, time::Duration};

use crate::state::PlayingContextQuery;
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};


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
    pub card_index: i32
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioCard {
    pub index: i32,
    pub name: String,
    pub description: String,
    pub pcm_devices: Vec<PcmOutputDevice>,
    pub mixers: Vec<CardMixer>
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
pub enum PlayerCommand {
    // Player commands
    Next,
    Prev,
    Pause,
    Play,
    PlayItem(String),
    RemovePlaylistItem(String),
    RandomToggle,
    Rewind(i8),
    LoadPlaylist(String),
    LoadAlbum(String),
    LoadSong(String),
    AddSongToQueue(String),
    ClearQueue,
    SaveQueueAsPlaylist(String),

    // Query commands
    QueryCurrentSong,
    QueryCurrentPlayerInfo,
    QueryCurrentStreamerState,
    QueryCurrentPlayingContext(PlayingContextQuery),
    QueryDynamicPlaylists(Vec<String>, u32, u32),
    QueryPlaylistItems(String, usize),
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
    // Metadata commands
    RescanMetadata(String),
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

#[derive(Debug, Eq, PartialEq, Clone, Hash, Copy, EnumIter, EnumString, Serialize, Deserialize)]
pub enum PlayerType {
    SPF,
    MPD,
    LMS,
    RSP,
}
impl Display for PlayerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SPF => f.write_str("Spotify"),
            Self::MPD => f.write_str("Music Player Deamon"),
            Self::LMS => f.write_str("Logitech Media Server"),
            Self::RSP => f.write_str("RSPlayer - experimental"),
        }
    }
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
