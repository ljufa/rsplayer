use std::time::Duration;

use num_derive::{FromPrimitive, ToPrimitive};

use strum_macros::{EnumIter, EnumProperty};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamerStatus {
    pub source_player: PlayerType,
    pub selected_audio_output: AudioOut,
    pub dac_status: DacStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DacStatus {
    pub volume: u8,
    pub filter: FilterType,
    pub sound_sett: u8,
    pub gain: GainLevel,
    pub heavy_load: bool,
    pub muted: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    RandomToggle,
    Rewind(i8),
    LoadPlaylist(String)
}

#[derive(Debug, Clone, Eq, PartialEq, EnumProperty, Serialize, Deserialize)]
#[strum(serialize_all = "title_case")]
pub enum StatusChangeEvent {
    Playing,
    Paused,
    Stopped,
    SwitchedToNextTrack,
    SwitchedToPrevTrack,
    CurrentTrackInfoChanged(CurrentTrackInfo),
    StreamerStatusChanged(StreamerStatus),
    PlaylistLoaded(String),
    PlayerInfoChanged(PlayerInfo),
    Error(String),
    Shutdown
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

impl CurrentTrackInfo {
    pub fn info_string(&self) -> Option<String> {
        let mut result = "".to_string();
        if let Some(artist) = self.artist.as_ref() {
            result.push_str(artist.as_str());
            result.push('-');
        }
        if let Some(album) = self.album.as_ref() {
            result.push_str(album.as_str());
            result.push('-');
        }
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        }
        if result.is_empty() {
            if let Some(filename) = self.filename.as_ref() {
                result.push_str(filename.as_str());
            }
        }
        if !result.is_empty() {
            Some(result)
        } else {
            None
        }
    }
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
            dur_to_string(self.time.0),
            dur_to_string(self.time.1)
        );
    }
}
fn dur_to_string(duration: Duration) -> String {
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

impl Default for StreamerStatus {
    fn default() -> Self {
        Self {
            source_player: PlayerType::MPD,
            selected_audio_output: AudioOut::SPKR,
            dac_status: DacStatus::default(),
        }
    }
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
