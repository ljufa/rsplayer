use core::default::Default;
use core::option::Option;

use core::time::Duration;

use serde::{Deserialize, Serialize};


use crate::common::MetadataLibraryItem;
use crate::{
    common::{PlaybackMode, Volume},
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
    pub audio_format_rate: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_bit: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_format_channels: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateChangeEvent {
    CurrentSongEvent(Song),
    CurrentQueueEvent(Option<PlaylistPage>),
    VolumeChangeEvent(Volume),
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
    PlaybackStateEvent(PlayerState),
    PlaybackModeChangedEvent(PlaybackMode),
    VUEvent(u8, u8),
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
    ERROR(String),
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

    #[must_use]
    pub fn format_total_time(&self) -> String {
        crate::common::dur_to_string(&self.total_time)
    }
}
