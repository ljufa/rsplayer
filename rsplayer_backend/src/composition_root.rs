use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use log::{error, info};
use tokio::sync::{broadcast, mpsc};

use api_models::common::{SystemCommand, UserCommand};
use api_models::state::StateChangeEvent;
use rsplayer_config::ArcConfiguration;
use rsplayer_hardware::audio_device::audio_service::{ArcAudioInterfaceSvc, AudioInterfaceService};
use rsplayer_hardware::usb::{ArcUsbService, UsbService};
use rsplayer_metadata::album_repository::FjallAlbumRepository;
use rsplayer_metadata::loudness_repository::FjallLoudnessRepository;
use rsplayer_metadata::loudness_service::LoudnessService;
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::play_statistic_repository::FjallPlayStatisticsRepository;
use rsplayer_metadata::playlist_service::PlaylistService;
use rsplayer_metadata::ports::{
    album_repository::ArcAlbumRepository, loudness_repository::ArcLoudnessRepository,
    play_statistics_repository::ArcPlayStatisticsRepository, song_repository::ArcSongRepository,
};
use rsplayer_metadata::queue_service::QueueService;
use rsplayer_metadata::song_repository::FjallSongRepository;
use rsplayer_playback::rsp::player_service::PlayerService;

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
    pub config: ArcConfiguration,
    pub shared_db: Arc<fjall::Database>,

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
}

pub enum BuildOutcome {
    Ready(Box<AppContainer>),
    Degraded(anyhow::Error),
}

pub fn build(config: ArcConfiguration, shared_db: Arc<fjall::Database>) -> BuildOutcome {
    let song_repository: ArcSongRepository = Arc::new(FjallSongRepository::new(&shared_db));
    let album_repository: ArcAlbumRepository = Arc::new(FjallAlbumRepository::new(&shared_db));
    let play_statistics_repository: ArcPlayStatisticsRepository =
        Arc::new(FjallPlayStatisticsRepository::new(&shared_db));
    let loudness_repository: ArcLoudnessRepository = Arc::new(FjallLoudnessRepository::new(&shared_db));

    let metadata_service = Arc::new(
        MetadataService::new(
            shared_db.clone(),
            &config.get_settings().metadata_settings,
            song_repository.clone(),
            album_repository.clone(),
            play_statistics_repository.clone(),
        )
        .expect("Failed to start metadata service"),
    );
    info!("Metadata service successfully created.");

    let playlist_service = Arc::new(PlaylistService::new(&shared_db));
    info!("Playlist service successfully created.");

    let queue_service = Arc::new(QueueService::new(
        &shared_db,
        song_repository.clone(),
        play_statistics_repository.clone(),
    ));
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
    let initial_volume = config
        .get_settings()
        .volume_ctrl_settings
        .saved_volume
        .unwrap_or(0);
    let current_volume = Arc::new(AtomicU8::new(initial_volume));

    let audio_service: ArcAudioInterfaceSvc =
        match AudioInterfaceService::new(&config, usb_service.clone(), current_volume.clone()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!("Audio service interface can't be created. error: {e}");
            return BuildOutcome::Degraded(e);
        }
    };
    info!("Audio interface service successfully created.");

    let user_commands = ChannelPair::<UserCommand>::new(5);
    let system_commands = ChannelPair::<SystemCommand>::new(5);
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

    let player_service = Arc::new(PlayerService::new(
        &shared_db,
        &config.get_settings(),
        current_volume,
        metadata_service.clone(),
        queue_service.clone(),
        state_changes_tx.clone(),
        loudness_service,
    ));
    info!("Player service successfully created.");

    BuildOutcome::Ready(Box::new(AppContainer {
        config,
        shared_db,
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_models::common::PlayerCommand;
    use rsplayer_config::Configuration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn build_wires_services_and_channels() {
        let tmp = TempDir::new().expect("temp dir");
        let db = Arc::new(
            fjall::Database::builder(tmp.path().join("test.db"))
                .open()
                .expect("open temp db"),
        );
        let config = Arc::new(Configuration::new(&db));

        let mut container = match build(config, db) {
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
