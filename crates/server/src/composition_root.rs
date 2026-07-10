//! Dependency wiring — the only place concrete implementations meet.
//!
//! [`build_app_container`] constructs repositories (fjall), services, the
//! command/state channels, and — when multiroom is enabled — the `SyncTee`
//! plus the pieces the sync service needs. Returns `Degraded` instead of a
//! container when the audio device fails, so `lib.rs` can fall back to the
//! settings-only server.

use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::Arc;

use log::{error, info};
use tokio::sync::{broadcast, mpsc};

use api_models::common::{MultiroomCommand, SystemCommand, UserCommand};
use api_models::state::StateChangeEvent;
use config::ArcConfiguration;
use hardware::audio_device::audio_service::{ArcAudioInterfaceSvc, AudioInterfaceService};
use hardware::usb::{ArcUsbService, UsbService};
use metadata::album_repository::FjallAlbumRepository;
use metadata::loudness_repository::FjallLoudnessRepository;
use metadata::loudness_service::LoudnessService;
use metadata::metadata_service::MetadataService;
use metadata::play_statistic_repository::FjallPlayStatisticsRepository;
use metadata::playlist_service::PlaylistService;
use metadata::ports::{
    album_repository::ArcAlbumRepository, loudness_repository::ArcLoudnessRepository,
    play_statistics_repository::ArcPlayStatisticsRepository, song_repository::ArcSongRepository,
};
use metadata::queue_service::QueueService;
use metadata::song_repository::FjallSongRepository;
use playback::rsp::player_service::PlayerService;
use playback::rsp::tee::{SyncTee, TeeEvent};

pub struct ChannelPair<T> {
    pub tx: mpsc::Sender<T>,
    pub rx: mpsc::Receiver<T>,
}

impl<T> ChannelPair<T> {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self { tx, rx }
    }

    pub fn split(self) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
        (self.tx, self.rx)
    }
}

pub struct AppContainer {
    pub song_repository: ArcSongRepository,
    pub album_repository: ArcAlbumRepository,
    pub loudness_repository: ArcLoudnessRepository,

    pub metadata_service: Arc<MetadataService>,
    pub playlist_service: Arc<PlaylistService>,
    pub queue_service: Arc<QueueService>,
    pub player_service: Arc<PlayerService>,

    pub audio_service: ArcAudioInterfaceSvc,
    pub usb_service: Option<ArcUsbService>,

    pub state_changes_tx: broadcast::Sender<StateChangeEvent>,
    pub user_commands: ChannelPair<UserCommand>,
    pub system_commands: ChannelPair<SystemCommand>,
    pub multiroom_commands: ChannelPair<MultiroomCommand>,
    /// True while this instance plays as a grouped multiroom follower;
    /// local transport commands are rejected while set.
    pub multiroom_follower_active: Arc<AtomicBool>,
    /// Audio-side multiroom plumbing — `Some` when multiroom is enabled.
    pub multiroom: Option<MultiroomParts>,
}

/// Everything the sync service needs beyond the command/event channels.
pub struct MultiroomParts {
    pub tee_rx: mpsc::Receiver<TeeEvent>,
    pub tee_active: Arc<AtomicBool>,
    pub sink_params: sync::follower::SinkParams,
}

pub enum BuildOutcome {
    Ready(Box<AppContainer>),
    Degraded(anyhow::Error),
}

#[allow(clippy::too_many_lines)]
pub fn build_app_container(config: &ArcConfiguration, shared_db: &Arc<fjall::Database>) -> BuildOutcome {
    let song_repository: ArcSongRepository = Arc::new(FjallSongRepository::new(shared_db));
    let album_repository: ArcAlbumRepository = Arc::new(FjallAlbumRepository::new(shared_db));
    let play_statistics_repository: ArcPlayStatisticsRepository = Arc::new(FjallPlayStatisticsRepository::new(shared_db));
    let loudness_repository: ArcLoudnessRepository = Arc::new(FjallLoudnessRepository::new(shared_db));

    let metadata_service = MetadataService::new(
        shared_db.clone(),
        &config.get_settings().metadata_settings,
        song_repository.clone(),
        album_repository.clone(),
        play_statistics_repository.clone(),
    )
    .expect("Failed to start metadata service");

    info!("Metadata service successfully created.");

    let playlist_service = PlaylistService::new(shared_db);
    info!("Playlist service successfully created.");

    let queue_service = QueueService::new(shared_db, song_repository.clone(), play_statistics_repository.clone());
    info!("Queue service successfully created.");

    let usb_settings = config.get_settings().usb_settings;
    let usb_service: Option<ArcUsbService> = if usb_settings.enabled {
        let service = Arc::new(UsbService::new(usb_settings.baud_rate));
        let _ = service.try_reconnect();
        Some(service)
    } else {
        None
    };

    // Shared volume state. Initialized from the saved volume so software-gain
    // playback starts at the user's last setting rather than zero. Updated by
    // VolumeChangeEvent in `PlayerService` and read by the playback writers
    // when `VolumeCrtlType::Software` is active.
    let initial_volume = config.get_settings().volume_ctrl_settings.saved_volume.unwrap_or(0);
    let current_volume = Arc::new(AtomicU8::new(initial_volume));

    let audio_service: ArcAudioInterfaceSvc = match AudioInterfaceService::new(config, usb_service.clone(), current_volume.clone()) {
        Ok(s) => s,
        Err(e) => {
            error!("Audio service interface can't be created. error: {e}");
            return BuildOutcome::Degraded(e);
        }
    };
    info!("Audio interface service successfully created.");

    let user_commands = ChannelPair::<UserCommand>::new(5);
    let system_commands = ChannelPair::<SystemCommand>::new(5);
    let multiroom_commands = ChannelPair::<MultiroomCommand>::new(16);
    let multiroom_follower_active = Arc::new(AtomicBool::new(false));
    let (state_changes_tx, _) = broadcast::channel(64);

    let loudness_service = LoudnessService::new(
        loudness_repository.clone(),
        song_repository.clone(),
        config.get_settings().metadata_settings.effective_directories(),
    );
    if config.get_settings().rs_player_settings.loudness_normalization_enabled {
        loudness_service.start();
        info!("Loudness scan service started.");
    } else {
        info!("Loudness scan service disabled (loudness normalization is off).");
    }

    let settings = config.get_settings();
    let (sync_tee, sync_tee_rx) = if settings.multiroom_settings.enabled {
        let (tee, rx) = SyncTee::new(settings.multiroom_settings.buffer_ms);
        (Some(tee), Some(rx))
    } else {
        (None, None)
    };

    let player_service = PlayerService::new(
        shared_db,
        &settings,
        current_volume.clone(),
        metadata_service.clone(),
        queue_service.clone(),
        state_changes_tx.clone(),
        loudness_service,
        sync_tee.clone(),
    );
    info!("Player service successfully created.");

    let multiroom = match (sync_tee, sync_tee_rx) {
        (Some(tee), Some(tee_rx)) => Some(MultiroomParts {
            tee_rx,
            tee_active: tee.active_flag(),
            sink_params: sync::follower::SinkParams {
                audio_device: settings.alsa_settings.output_device.name.clone(),
                rsp_settings: settings.rs_player_settings.clone(),
                software_gain: if settings.volume_ctrl_settings.ctrl_device == api_models::common::VolumeCrtlType::Software {
                    Some(current_volume)
                } else {
                    None
                },
                vu_meter_enabled: settings.rs_player_settings.vu_meter_enabled,
                dsp_handle: player_service.dsp_handle(),
                latency_offset_ms: settings.multiroom_settings.output_latency_offset_ms,
                changes_tx: state_changes_tx.clone(),
            },
        }),
        _ => None,
    };

    BuildOutcome::Ready(Box::new(AppContainer {
        song_repository,
        album_repository,
        loudness_repository,
        metadata_service,
        playlist_service,
        queue_service,
        player_service,
        audio_service,
        usb_service,
        state_changes_tx,
        user_commands,
        system_commands,
        multiroom_commands,
        multiroom_follower_active,
        multiroom,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_models::common::PlayerCommand;
    use config::Configuration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn build_wires_services_and_channels() {
        let tmp = TempDir::new().expect("temp dir");
        let db = Arc::new(fjall::Database::builder(tmp.path().join("test.db")).open().expect("open temp db"));
        let config = Configuration::new(&db, api_models::settings::Settings::default());

        let mut container = match build_app_container(&config, &db) {
            BuildOutcome::Ready(c) => c,
            BuildOutcome::Degraded(e) => panic!("expected Ready, got Degraded: {e}"),
        };

        assert!(container.usb_service.is_none(), "default settings should not enable USB");

        let cmd = UserCommand::Player(PlayerCommand::Pause);
        container.user_commands.tx.send(cmd.clone()).await.expect("send user command");
        let received = container.user_commands.rx.recv().await.expect("rx closed");
        assert_eq!(received, cmd);

        let sys_cmd = SystemCommand::PowerOff;
        container
            .system_commands
            .tx
            .send(sys_cmd.clone())
            .await
            .expect("send system command");
        let received_sys = container.system_commands.rx.recv().await.expect("rx closed");
        assert_eq!(received_sys, sys_cmd);
    }
}
