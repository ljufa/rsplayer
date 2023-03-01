extern crate env_logger;
#[macro_use]
extern crate log;

use std::panic;
use std::sync::Arc;

use rsplayer_playback::player_service::PlayerService;
use tokio::signal::unix::{Signal, SignalKind};
use tokio::sync::broadcast;
use tokio::{select, spawn};

use rsplayer_config::Configuration;

use rsplayer_hardware::audio_device::audio_service::AudioInterfaceService;
use rsplayer_hardware::oled::st7920;
use rsplayer_hardware::input::ir_lirc;
use rsplayer_hardware::input::volume_rotary;

use rsplayer_metadata::metadata::MetadataService;



mod command_handler;
mod server_warp;

mod status;

#[allow(clippy::redundant_pub_crate)]
#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Starting RSPlayer!");

    let config = Arc::new(Configuration::new());

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to create signal future");

    let metadata_service =
        MetadataService::new(&config.get_settings().metadata_settings);
    if let Err(e) = &metadata_service {
        error!("Metadata service can't be created. error: {}", e);
        start_degraded(
            &mut term_signal,
            &anyhow::format_err!("Failed to start metaservice"),
            &config,
        )
        .await;
    }
    let metadata_service = Arc::new(metadata_service.unwrap());

    let ai_service = AudioInterfaceService::new(&config);

    if let Err(e) = &ai_service {
        error!("Audio service interface can't be created. error: {}", e);
        start_degraded(&mut term_signal, e, &config).await;
    }
    let ai_service = Arc::new(ai_service.unwrap());
    info!("Audio interface service successfully created.");

    let player_service = PlayerService::new(&config, metadata_service.clone());
    if let Err(e) = &player_service {
        error!("Player service can't be created. error: {}", e);
        start_degraded(&mut term_signal, e, &config).await;
    }

    let player_service = Arc::new(player_service.unwrap());
    info!("Player service successfully created.");

    let (player_commands_tx, player_commands_rx) = tokio::sync::mpsc::channel(10);

    let (system_commands_tx, system_commands_rx) = tokio::sync::mpsc::channel(10);

    let (state_changes_tx, _) = broadcast::channel(20);

    let (http_server_future, websocket_future) = server_warp::start(
        state_changes_tx.subscribe(),
        player_commands_tx.clone(),
        system_commands_tx.clone(),
        &config,
        player_service.clone(),
    );
    if config.get_settings().auto_resume_playback {
        player_service.get_current_player().play_from_current_queue_song();
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

        _ = spawn(status::monitor(player_service.clone(), state_changes_tx.clone())) => {
            error!("Exit from status monitor thread.");
        }

        _ = spawn(command_handler::handle_player_commands(
                player_service.clone(),
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
async fn start_degraded(
    term_signal: &mut Signal,
    error: &anyhow::Error,
    config: &Arc<Configuration>,
) {
    warn!("Starting server in degraded mode.");
    let http_server_future = server_warp::start_degraded(config, error);
    select! {
        _ = http_server_future => {}

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    }
}
