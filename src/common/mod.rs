use core::result;
use std::time::Duration;

use failure::Error;
use futures::Stream;
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

pub const DPLAY_CONFIG_DIR_PATH: &str = ".dplay/";

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CurrentTrackInfo {
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
}

impl CurrentTrackInfo {
    pub fn info_string(&self) -> Option<String> {
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
        if result.is_empty() {
            if let Some(filename) = self.filename.as_ref() {
                result.push_str(filename.as_str());
            }
        }
        if result.len() > 0 {
            Some(result)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInfo {
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
    pub time: (Duration, Duration),
}
impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            state: None,
            random: None,
            audio_format_rate: None,
            audio_format_bit: None,
            audio_format_channels: None,
            time: (Duration::ZERO, Duration::ZERO),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamerStatus {
    pub source_player: PlayerType,
    pub selected_audio_output: AudioOut,
    pub dac_status: DacStatus,
}

impl Default for StreamerStatus {
    fn default() -> Self {
        Self {
            source_player: PlayerType::MPD,
            selected_audio_output: AudioOut::SPKR,
            dac_status: DacStatus::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DacStatus {
    pub volume: u8,
    pub filter: FilterType,
    pub sound_sett: u8,
    pub gain: GainLevel,
    pub heavy_load: bool,
    pub muted: bool,
}

impl Default for DacStatus {
    fn default() -> Self {
        Self {
            volume: 180,
            filter: FilterType::SharpRollOff,
            gain: GainLevel::V375,
            muted: false,
            sound_sett: 5,
            heavy_load: true,
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
    Playing,
    Paused,
    Stopped,
    SwitchedToNextTrack,
    SwitchedToPrevTrack,
    CurrentTrackInfoChanged(CurrentTrackInfo),
    StreamerStatusChanged(StreamerStatus),
    PlayerInfoChanged(PlayerInfo),
    Error(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
