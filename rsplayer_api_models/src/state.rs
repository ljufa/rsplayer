use core::default::Default;
use core::option::Option;
use core::option::Option::None;
use core::time::Duration;

use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::EnumProperty;

use crate::{
    common::PlayerType,
    player::Song,
    playlist::{DynamicPlaylistsPage, PlaylistPage},
};

// todo move somewhere else
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayingContextType {
    Playlist {
        snapshot_id: String,
        description: Option<String>,
        public: Option<bool>,
    },
    Collection,
    Album {
        artists: Vec<String>,
        release_date: String,
        label: Option<String>,
        genres: Vec<String>,
    },
    Artist {
        genres: Vec<String>,
        popularity: u32,
        followers: u32,
        description: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
pub struct PlayingContext {
    pub id: String,
    pub name: String,
    pub player_type: PlayerType,
    pub context_type: PlayingContextType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_page: Option<PlaylistPage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
pub enum PlayingContextQuery {
    WithSearchTerm(String, usize),
    CurrentSongPage,
    IgnoreSongs,
}
// end todo

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamerState {
    pub selected_audio_output: AudioOut,
    pub volume_state: VolumeState,
}

#[derive(Debug, Clone, Eq, PartialEq, EnumProperty, Serialize, Deserialize)]
#[strum(serialize_all = "title_case")]
pub enum StateChangeEvent {
    CurrentSongEvent(Song),
    CurrentPlayingContextEvent(PlayingContext),
    StreamerStateEvent(StreamerState),
    PlayerInfoEvent(PlayerInfo),
    SongTimeEvent(SongProgress),
    ErrorEvent(String),
    DynamicPlaylistsPageEvent(Vec<DynamicPlaylistsPage>),
    PlaylistItemsEvent(Vec<Song>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeState {
    pub volume: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SongProgress {
    pub total_time: Duration,
    pub current_time: Duration,
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

#[derive(
    Debug, Eq, PartialEq, Clone, Hash, Copy, FromPrimitive, ToPrimitive, Serialize, Deserialize,
)]
pub enum AudioOut {
    SPKR,
    HEAD,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            state: None,
            random: None,
            audio_format_rate: None,
            audio_format_bit: None,
            audio_format_channels: None,
        }
    }
}

impl SongProgress {
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
            volume_state: VolumeState::default(),
        }
    }
}
