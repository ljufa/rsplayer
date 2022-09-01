use std::{time::Duration, fmt::Display};

use num_derive::{FromPrimitive, ToPrimitive};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};
use crate::state::PlayingContextQuery;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolumeCrtlType {
    Dac, Alsa
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Volume {
    pub step: i64,
    pub min: i64,
    pub max: i64,
    pub current: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum Command {
    VolUp,
    VolDown,
    SetVol(u8),
    Next,
    Prev,
    Pause,
    Play,
    PlayItem(String),
    RemovePlaylistItem(String),
    SwitchToPlayer(PlayerType),
    PowerOff,
    ChangeAudioOutput,
    RandomToggle,
    Rewind(i8),
    LoadPlaylist(String),
    LoadAlbum(String),
    LoadSong(String),
    AddSongToQueue(String),
    QueryCurrentSong,
    QueryCurrentPlayerInfo,
    QueryCurrentStreamerState,
    QueryCurrentPlayingContext(PlayingContextQuery),
    QueryDynamicPlaylists(Vec<String>, u32, u32),
    QueryPlaylistItems(String),
    
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, EnumIter, IntoStaticStr)]
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
}
impl Display for PlayerType{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self{
            PlayerType::SPF => f.write_str("Spotify"),
            PlayerType::MPD => f.write_str("Music Player Deamon"),
            PlayerType::LMS => f.write_str("Logitech Media Server"),
        }
    }
}

pub fn dur_to_string(duration: &Duration) -> String {
    let mut result = "00:00:00".to_string();
    let secs = duration.as_secs();
    if secs > 0 {
        let seconds = secs % 60;
        let minutes = (secs / 60) % 60;
        let hours = (secs / 60) / 60;
        result = format!("{:0>2}:{:0>2}:{:0>2}", hours, minutes, seconds);
    }
    result
}
