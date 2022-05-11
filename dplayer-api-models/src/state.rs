use crate::player::Song;
use core::default::Default;
use core::option::Option;
use core::option::Option::None;
use core::time::Duration;
use num_derive::{FromPrimitive, ToPrimitive};
use strum_macros::EnumProperty;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayingContextType {
    Playlist,
    Collection,
    Album,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayingContext {
    pub _type: Option<PlayingContextType>,
    pub name: String,
    pub id: String,
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

impl PlayerInfo {
    pub fn format_time(&self) -> String {
        return format!(
            "{} / {}",
            crate::common::dur_to_string(self.time.0),
            crate::common::dur_to_string(self.time.1)
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamerState {
    pub selected_audio_output: AudioOut,
    pub volume_state: VolumeState,
}

impl Default for StreamerState {
    fn default() -> Self {
        Self {
            selected_audio_output: AudioOut::SPKR,
            volume_state: VolumeState::default(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, EnumProperty, Serialize, Deserialize)]
#[strum(serialize_all = "title_case")]
pub enum StateChangeEvent {
    Playing,
    Paused,
    Stopped,
    SwitchedToNextTrack,
    SwitchedToPrevTrack,
    CurrentTrackInfoChanged(Song),
    StreamerStateChanged(StreamerState),
    PlaylistLoaded(String),
    PlayerInfoChanged(PlayerInfo),
    Error(String),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeState {
    pub volume: i64,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self { volume: 180 }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PlayerState {
    PLAYING,
    PAUSED,
    STOPPED,
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct LastState {
    pub current_track_info: Option<Song>,
    pub player_info: Option<PlayerInfo>,
    pub streamer_state: Option<StreamerState>,
}

#[derive(
    Debug, Eq, PartialEq, Clone, Hash, Copy, FromPrimitive, ToPrimitive, Serialize, Deserialize,
)]
pub enum AudioOut {
    SPKR,
    HEAD,
}
