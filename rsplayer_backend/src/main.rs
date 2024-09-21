extern crate env_logger;
#[macro_use]
extern crate log;

use std::panic;
use std::sync::Arc;
#[cfg(debug_assertions)]
use std::time::Duration;

use env_logger::Env;

use tokio::signal::unix::{Signal, SignalKind};
use tokio::sync::broadcast;
use tokio::{select, spawn};

use album_repository::AlbumRepository;
use rsplayer_config::Configuration;
use rsplayer_hardware::audio_device::audio_service::AudioInterfaceService;
use rsplayer_hardware::input::ir_lirc;
use rsplayer_hardware::input::volume_rotary;
use rsplayer_hardware::oled::st7920;
use rsplayer_metadata::album_repository;
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::play_statistic_repository::PlayStatisticsRepository;
use rsplayer_metadata::playlist_service::PlaylistService;
use rsplayer_metadata::queue_service::QueueService;
use rsplayer_metadata::song_repository::SongRepository;
use rsplayer_playback::rsp::player_service::PlayerService;

mod command_handler;
mod server_warp;

#[allow(clippy::redundant_pub_crate, clippy::too_many_lines)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    #[cfg(debug_assertions)]
    console_subscriber::ConsoleLayer::builder()
        .retention(Duration::from_secs(60))
        .server_addr(([0, 0, 0, 0], 6669))
        .init();
    let version = env!("CARGO_PKG_VERSION");
    info!("Starting RSPlayer {version}.");
    info!(
        r#" 
        -------------------------------------------------------------------------

            ██████╗ ███████╗██████╗ ██╗      █████╗ ██╗   ██╗███████╗██████╗
            ██╔══██╗██╔════╝██╔══██╗██║     ██╔══██╗╚██╗ ██╔╝██╔════╝██╔══██╗
            ██████╔╝███████╗██████╔╝██║     ███████║ ╚████╔╝ █████╗  ██████╔╝
            ██╔══██╗╚════██║██╔═══╝ ██║     ██╔══██║  ╚██╔╝  ██╔══╝  ██╔══██╗
            ██║  ██║███████║██║     ███████╗██║  ██║   ██║   ███████╗██║  ██║
            ╚═╝  ╚═╝╚══════╝╚═╝     ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝
            /     /       
            by https://github.com/ljufa/rsplayer
        
        -------------------------------------------------------------------------
    "#
    );

    let config = Arc::new(Configuration::new());
    info!("Configuration successfully loaded.");

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate()).expect("failed to create signal future");

    let album_repository = Arc::new(AlbumRepository::default());
    let song_repository = Arc::new(SongRepository::default());
    let statistics_repository = Arc::new(PlayStatisticsRepository::default());
    let metadata_service = Arc::new(
        MetadataService::new(
            &config.get_settings().metadata_settings,
            song_repository.clone(),
            album_repository.clone(),
            statistics_repository.clone(),
        )
        .expect("Failed to start metadata service"),
    );
    info!("Metadata service successfully created.");

    let playlist_service = Arc::new(PlaylistService::new(&config.get_settings().playlist_settings));
    info!("Playlist service successfully created.");
    let queue_service = Arc::new(QueueService::new(
        &config.get_settings().playback_queue_settings,
        song_repository.clone(),
        statistics_repository.clone(),
    ));
    info!("Queue service successfully created.");

    let ai_service = AudioInterfaceService::new(&config);
    if let Err(e) = &ai_service {
        error!("Audio service interface can't be created. error: {}", e);
        start_degraded(&mut term_signal, e, &config).await;
    }
    let ai_service = Arc::new(ai_service.unwrap());
    info!("Audio interface service successfully created.");

    let (player_commands_tx, player_commands_rx) = tokio::sync::mpsc::channel(5);

    let (system_commands_tx, system_commands_rx) = tokio::sync::mpsc::channel(5);

    let (state_changes_tx, _) = broadcast::channel(20);

    let player_service = Arc::new(PlayerService::new(
        &config.get_settings(),
        metadata_service.clone(),
        queue_service.clone(),
        state_changes_tx.clone()
    ));
    info!("Player service successfully created.");

    let (http_server_future, https_server_future, websocket_future) = server_warp::start(
        state_changes_tx.subscribe(),
        player_commands_tx.clone(),
        system_commands_tx.clone(),
        &config,
    );

    if config.get_settings().auto_resume_playback {
        player_service.play_from_current_queue_song();
    }

    select! {
        _ = spawn(ir_lirc::listen(player_commands_tx.clone(), system_commands_tx.clone(), config.clone())) => {
            error!("Exit from IR Command thread.");
        }

        _ = spawn(volume_rotary::listen(system_commands_tx.clone(), config.clone())) => {
            error!("Exit from Volume control thread.");
        }

        _ = spawn(st7920::write(state_changes_tx.subscribe(), config.clone())) => {
            error!("Exit from OLED writer thread.");
        }

        _ = spawn(command_handler::handle_user_commands(
                player_service.clone(),
                metadata_service.clone(),
                playlist_service.clone(),
                queue_service.clone(),
                album_repository.clone(),
                song_repository.clone(),
                config.clone(),
                player_commands_rx,
                state_changes_tx.clone())) => {
            error!("Exit from command handler thread.");
        }
        _ = spawn(command_handler::handle_system_commands(
                ai_service,
                metadata_service,
                config.clone(),
                system_commands_rx,
                state_changes_tx.clone())) => {
            error!("Exit from command handler thread.");
        }

        _ = spawn(http_server_future) => {}
        _ = spawn(https_server_future) => {}

        _ = spawn(websocket_future) => {
            error!("Exit from websocket thread.");
        }

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    };

    info!("RSPlayer shutdown completed.");
}

#[allow(clippy::redundant_pub_crate)]
async fn start_degraded(term_signal: &mut Signal, error: &anyhow::Error, config: &Arc<Configuration>) {
    warn!("Starting server in degraded mode.");
    let http_server_future = server_warp::start_degraded(config, error);
    select! {
        () = http_server_future => {}

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    }
}
