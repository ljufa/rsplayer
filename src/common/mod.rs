use core::result;
use std::fmt::format;

use failure::Error;
use num_derive::{FromPrimitive, ToPrimitive};
use strum_macros::EnumIter;

use crate::config::{DacStatus, StreamerStatus};

pub const DPLAY_CONFIG_DIR_PATH: &str = ".dplay/";

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PlayerStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<PlayerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_bit: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<(String, String)>,
}

impl PlayerStatus {
    pub fn song_info_string(&self) -> Option<String> {
        let mut result = "".to_string();
        if let Some(artist) = self.artist.as_ref() {
            result.push_str(artist.as_str());
            result.push_str("-");
        }
        if let Some(album) = self.album.as_ref() {
            result.push_str(album.as_str());
            result.push_str("-");
        }
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        }
        if result.len() > 0 {
            Some(result)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PlayerState {
    PLAYING,
    PAUSED,
    STOPPED,
}

#[derive(
    Debug, Eq, PartialEq, Clone, Hash, Copy, FromPrimitive, ToPrimitive, Serialize, Deserialize,
)]
pub enum AudioOut {
    SPKR,
    HEAD,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Command {
    VolUp,
    VolDown,
    Filter(FilterType),
    SetVol(u8),
    Sound(u8),
    Gain(GainLevel),
    Hload(bool),
    Dsd(bool),
    Next,
    Prev,
    Pause,
    Play,
    TogglePlayer,
    SwitchToPlayer(PlayerType),
    PowerOff,
    ChangeAudioOutput,
    Rewind(i8),
}

#[derive(Debug, Clone, Eq, PartialEq, EnumProperty, Serialize, Deserialize)]
#[strum(serialize_all = "title_case")]
pub enum CommandEvent {
    DplayStarted(String),
    #[strum(props(config_key = "volume"))]
    VolumeChanged(u8),
    Playing,
    Paused,
    Stopped,
    SwitchedToNextTrack,
    SwitchedToPrevTrack,
    #[strum(props(config_key = "player_type"))]
    PlayerChanged(PlayerType),
    #[strum(props(config_key = "filter"))]
    FilterChanged(FilterType),
    #[strum(props(config_key = "sound_setting"))]
    SoundChanged(u8),
    GainChanged(GainLevel),
    HiLoadChanged(bool),
    DsdChanged(bool),
    PlayerStatusChanged(PlayerStatus),
    DacStatusChanged(DacStatus),
    StreamerStatusChanged(StreamerStatus),
    Error(String),
    #[strum(props(config_key = "audio_out"))]
    AudioOutputChanged(AudioOut),
    ShuttingDown,
    Busy(Option<String>),
}

#[derive(
    Debug, Hash, Serialize, Clone, Copy, PartialEq, Eq, FromPrimitive, ToPrimitive, Deserialize,
)]
pub enum FilterType {
    SharpRollOff,
    SlowRollOff,
    ShortDelaySharpRollOff,
    ShortDelaySlowRollOff,
    SuperSlow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GainLevel {
    V25,
    V28,
    V375,
}

#[derive(
    Debug,
    Eq,
    PartialEq,
    Clone,
    Hash,
    Copy,
    EnumIter,
    FromPrimitive,
    ToPrimitive,
    Serialize,
    Deserialize,
)]
#[repr(u8)]
pub enum PlayerType {
    SPF = 1,
    MPD = 2,
    LMS = 3,
}
