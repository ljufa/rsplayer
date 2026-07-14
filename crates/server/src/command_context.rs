//! [`CommandContext`] — the service bundle passed to every command handler,
//! plus the `multiroom_follower_active` flag that locks local transport
//! while grouped.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::sync::broadcast::Sender;

use api_models::state::StateChangeEvent;
use config::ArcConfiguration;
use hardware::audio_device::audio_service::ArcAudioInterfaceSvc;
use hardware::usb::ArcUsbService;
use metadata::metadata_service::MetadataService;
use metadata::playlist_service::PlaylistService;
use metadata::ports::{
    album_repository::ArcAlbumRepository, loudness_repository::ArcLoudnessRepository, song_repository::ArcSongRepository,
};
use metadata::queue_service::QueueService;
use playback::rsp::player_service::PlayerService;

pub struct CommandContext {
    pub player_service: Arc<PlayerService>,
    pub metadata_service: Arc<MetadataService>,
    pub playlist_service: Arc<PlaylistService>,
    pub queue_service: Arc<QueueService>,
    pub album_repository: ArcAlbumRepository,
    pub song_repository: ArcSongRepository,
    pub loudness_repository: ArcLoudnessRepository,
    pub config_store: ArcConfiguration,
    /// True while this instance plays as a grouped multiroom follower.
    pub multiroom_follower_active: Arc<AtomicBool>,
    pub state_changes_sender: Sender<StateChangeEvent>,
}

impl CommandContext {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        player_service: Arc<PlayerService>,
        metadata_service: Arc<MetadataService>,
        playlist_service: Arc<PlaylistService>,
        queue_service: Arc<QueueService>,
        album_repository: ArcAlbumRepository,
        song_repository: ArcSongRepository,
        loudness_repository: ArcLoudnessRepository,
        config_store: ArcConfiguration,
        multiroom_follower_active: Arc<AtomicBool>,
        state_changes_sender: Sender<StateChangeEvent>,
    ) -> Self {
        Self {
            player_service,
            metadata_service,
            playlist_service,
            queue_service,
            album_repository,
            song_repository,
            loudness_repository,
            config_store,
            multiroom_follower_active,
            state_changes_sender,
        }
    }

    pub fn send_event(&self, event: StateChangeEvent) {
        let _ = self.state_changes_sender.send(event);
    }

    pub fn send_notification(&self, message: &str) {
        let _ = self
            .state_changes_sender
            .send(StateChangeEvent::NotificationSuccess(message.to_string()));
    }

    pub fn send_error(&self, message: &str) {
        let _ = self
            .state_changes_sender
            .send(StateChangeEvent::NotificationError(message.to_string()));
    }
}

pub struct SystemCommandContext {
    pub audio_service: ArcAudioInterfaceSvc,
    pub usb_service: Option<ArcUsbService>,
    pub config: ArcConfiguration,
    pub state_changes_sender: Sender<StateChangeEvent>,
    /// Present when running inside the desktop app: `RestartRSPlayer`
    /// signals here so the wrapper can relaunch the whole process,
    /// instead of `exit(1)` which relies on systemd to bring us back.
    pub restart_sender: Option<tokio::sync::mpsc::Sender<()>>,
}

impl SystemCommandContext {
    pub const fn new(
        audio_service: ArcAudioInterfaceSvc,
        usb_service: Option<ArcUsbService>,
        config: ArcConfiguration,
        state_changes_sender: Sender<StateChangeEvent>,
        restart_sender: Option<tokio::sync::mpsc::Sender<()>>,
    ) -> Self {
        Self {
            audio_service,
            usb_service,
            config,
            state_changes_sender,
            restart_sender,
        }
    }

    pub fn send_event(&self, event: StateChangeEvent) {
        let _ = self.state_changes_sender.send(event);
    }
}
