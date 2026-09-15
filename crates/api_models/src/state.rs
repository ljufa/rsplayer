//! Server → client events.
//!
//! [`StateChangeEvent`] is the single broadcast enum: every server-side change
//! (song, queue, volume, scan progress, notifications, multiroom state, VU
//! levels…) is published on one `tokio::sync::broadcast` channel and pushed to
//! all connected WebSocket clients as JSON. Queries in
//! [`crate::common::UserCommand`] are answered through these events too, so
//! every client converges on the same state regardless of who asked.

use core::default::Default;
use core::option::Option;

use core::time::Duration;

use serde::{Deserialize, Serialize};

use crate::podcast::{Episode, EpisodePage, Podcast, PodcastSearchResult};
use crate::common::MetadataLibraryItem;
use crate::{
    common::{PlaybackMode, Volume},
    player::Song,
    playlist::{Album, PlaylistPage, Playlists},
    stat::LibraryStats,
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

    /// Measured integrated loudness of the track in hundredths of LUFS
    /// (e.g. -1850 = -18.50 LUFS).  `None` if not yet analysed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_loudness_lufs: Option<i32>,

    /// Normalization gain applied to this track in hundredths of dB
    /// (e.g. 50 = +0.50 dB).  `None` when normalization is disabled or
    /// loudness has not been measured yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalization_gain_db: Option<i32>,
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
    /// Albums for a specific genre, returned on demand. (`genre_name`, albums)
    GenreAlbumsEvent(String, Vec<Album>),
    /// Albums for a specific decade, returned on demand. (`decade_label`, albums)
    DecadeAlbumsEvent(String, Vec<Album>),
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
    VuMeterEnabledEvent(bool),
    RSPlayerFirmwarePowerEvent(bool),
    LibraryStatsEvent(LibraryStats),
    MountStatusEvent(Vec<MountStatus>),
    MusicDirStatusEvent(Vec<MusicDirStatus>),
    ExternalMountsEvent(Vec<ExternalMount>),
    /// Reply to `StorageCommand::ListDirectories`.
    DirectoryListingEvent(DirectoryListing),
    MultiroomPeersEvent(Vec<MultiroomPeer>),
    MultiroomGroupEvent(MultiroomGroupState),
    /// All subscriptions, sent after any change and on `QueryPodcasts`.
    PodcastsEvent(Vec<Podcast>),
    PodcastSearchResultsEvent(Vec<PodcastSearchResult>),
    PodcastEpisodesEvent(EpisodePage),
    /// Progress/played state of one episode changed.
    PodcastEpisodeUpdatedEvent(Episode),
    /// The podcast worker is busy with a search/subscribe/refresh job.
    PodcastBusyEvent(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiroomPeer {
    pub endpoint_id: String,
    pub room_name: String,
    pub in_group: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MultiroomRole {
    /// Multiroom is disabled in settings.
    #[default]
    Off,
    /// Enabled, discoverable, not part of any group.
    Idle,
    Leader,
    Follower,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiroomGroupState {
    pub role: MultiroomRole,
    /// Room name of the leader when `role == Follower`.
    pub leader_name: Option<String>,
    /// Group members as seen by the leader (empty unless `role == Leader`).
    pub members: Vec<MultiroomPeer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountStatus {
    pub name: String,
    pub mount_point: String,
    pub is_mounted: bool,
    pub readable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicDirStatus {
    pub path: String,
    pub readable: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMount {
    pub source: String,
    pub mount_point: String,
    pub fs_type: String,
    pub readable: bool,
    pub writable: bool,
}

/// One level of server-side folders for the settings folder picker.
///
/// Broadcast like every event, so a client only uses the listing whose
/// `path` matches the path it asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DirectoryListing {
    /// The listed directory; empty for the library roots.
    pub path: String,
    /// Where "up" goes; `None` at a library root (back to the roots).
    pub parent: Option<String>,
    /// From the enclosing library root (or the filesystem root) down to `path`.
    pub breadcrumbs: Vec<PathCrumb>,
    pub entries: Vec<DirectoryEntry>,
    /// More sub-folders exist than were returned.
    pub truncated: bool,
    /// Why `path` could not be listed (missing, no permission, disabled…).
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathCrumb {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    /// Set for library roots ("Music", "Internal storage", …).
    pub label: Option<String>,
    pub subdirs: u32,
    pub audio_files: u32,
    /// The folder's contents can be listed.
    pub readable: bool,
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
