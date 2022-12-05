extern crate env_logger;
#[macro_use]
extern crate log;

use std::panic;
use std::sync::{Arc, Mutex};
use std::time::Duration;
// use std::time::Duration;

use api_models::common::PlayerCommand;
use tokio::signal::unix::{Signal, SignalKind};
use tokio::sync::broadcast;
use tokio::{select, spawn};

use config::Configuration;

use crate::audio_device::audio_service::AudioInterfaceService;
use crate::control::command_handler;
use crate::player::player_service::PlayerService;

mod audio_device;
mod common;
mod config;
mod control;
mod http_api;
mod mcu;
mod monitor;
mod player;

#[allow(clippy::redundant_pub_crate)]
#[tokio::main]
async fn main() {
    env_logger::init();
    // #[cfg(debug_assertions)]
    // console_subscriber::ConsoleLayer::builder()
    //     .retention(Duration::from_secs(60))
    //     .server_addr(([0, 0, 0, 0], 6669))
    //     .init();

    info!("Starting RSPlayer!");

    let config = Arc::new(Mutex::new(Configuration::new()));

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to create signal future");

    let ai_service = AudioInterfaceService::new(&config);

    if let Err(e) = &ai_service {
        error!("Audio service interface can't be created. error: {}", e);
        start_degraded(&mut term_signal, e, &config).await;
    }
    let ai_service = Arc::new(ai_service.unwrap());
    info!("Audio interface service successfully created.");

    let player_service = PlayerService::new(&config);
    if let Err(e) = &player_service {
        error!("Player service can't be created. error: {}", e);
        start_degraded(&mut term_signal, e, &config).await;
    }

    let player_service = Arc::new(Mutex::new(player_service.unwrap()));
    info!("Player service successfully created.");

    let (player_commands_tx, player_commands_rx) = tokio::sync::mpsc::channel(10);

    let (system_commands_tx, system_commands_rx) = tokio::sync::mpsc::channel(10);

    let (state_changes_tx, _) = broadcast::channel(20);

    let (http_server_future, websocket_future) = http_api::server_warp::start(
        state_changes_tx.subscribe(),
        player_commands_tx.clone(),
        system_commands_tx.clone(),
        &config,
        player_service.clone(),
    );

    // start/resume playing after start
    _ = player_commands_tx.send(PlayerCommand::Play).await;

    select! {
        _ = spawn(control::ir_lirc::listen(player_commands_tx.clone(), system_commands_tx.clone(), config.clone())) => {
            error!("Exit from IR Command thread.");
        }

        _ = spawn(control::volume_rotary::listen(system_commands_tx.clone(), config.clone())) => {
            error!("Exit from Volume control thread.");
        }

        _ = spawn(monitor::oled::write(state_changes_tx.subscribe(), config.clone())) => {
            error!("Exit from OLED writer thread.");
        }

        _ = spawn(monitor::status::monitor(player_service.clone(), state_changes_tx.clone())) => {
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
    error: &failure::Error,
    config: &Arc<Mutex<Configuration>>,
) {
    warn!("Starting server in degraded mode.");
    let http_server_future = http_api::server_warp::start_degraded(config, error);
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
