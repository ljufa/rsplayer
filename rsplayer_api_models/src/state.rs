use core::default::Default;
use core::option::Option;

use core::time::Duration;

use num_derive::ToPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::EnumProperty;

use crate::common::MetadataLibraryItem;
use crate::{
    common::Volume,
    player::Song,
    playlist::{PlaylistPage, Playlists},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
pub enum CurrentQueueQuery {
    WithSearchTerm(String, usize),
    CurrentSongPage,
    IgnoreSongs,
}
// end todo

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayerInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<PlayerState>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub random: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_rate: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_bit: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_channels: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamerState {
    pub selected_audio_output: AudioOut,
    pub volume_state: Volume,
}

#[derive(Debug, Clone, Eq, PartialEq, EnumProperty, Serialize, Deserialize)]
#[strum(serialize_all = "title_case")]
#[allow(clippy::large_enum_variant)]
pub enum StateChangeEvent {
    CurrentSongEvent(Song),
    CurrentQueueEvent(Option<PlaylistPage>),
    StreamerStateEvent(StreamerState),
    PlayerInfoEvent(PlayerInfo),
    SongTimeEvent(SongProgress),
    ErrorEvent(String),
    PlaylistsEvent(Playlists),
    PlaylistItemsEvent(Vec<Song>, usize),
    MetadataSongScanStarted,
    MetadataSongScanned(String),
    MetadataSongScanFinished(String),
    MetadataLocalItems(Vec<MetadataLibraryItem>),
    NotificationSuccess(String),
    NotificationError(String),

    FavoriteRadioStations(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SongProgress {
    pub total_time: Duration,
    pub current_time: Duration,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PlayerState {
    PLAYING,
    PAUSED,
    STOPPED,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash, Copy, ToPrimitive, Serialize, Deserialize)]
pub enum AudioOut {
    SPKR,
    HEAD,
}

impl SongProgress {
    #[must_use]
    pub fn format_time(&self) -> String {
        format!(
            "{} / {}",
            crate::common::dur_to_string(&self.current_time),
            crate::common::dur_to_string(&self.total_time)
        )
    }
}

impl Default for StreamerState {
    fn default() -> Self {
        Self {
            selected_audio_output: AudioOut::SPKR,
            volume_state: Volume::default(),
        }
    }
}
