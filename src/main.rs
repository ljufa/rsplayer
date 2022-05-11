extern crate serde_derive;
#[macro_use]
extern crate log;
extern crate env_logger;

mod audio_device;
mod common;
mod config;
mod control;
mod http_api;
mod mcu;
mod monitor;
mod player;

use crate::audio_device::audio_service::AudioInterfaceService;
use crate::control::command_handler;
use crate::player::player_service::PlayerService;

use api_models::common::Command;
use config::Configuration;

use std::panic;
use std::sync::{Arc, Mutex};
use tokio::signal::unix::{Signal, SignalKind};

use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Starting Dplayer!");

    let config = Arc::new(Mutex::new(config::Configuration::new()));

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to create signal future");

    let ai_service = AudioInterfaceService::new(config.clone());

    if let Err(e) = &ai_service {
        error!("Audio service interface can't be created. error: {}", e);
        start_degraded(&mut term_signal, config.clone()).await;
    }
    let ai_service = Arc::new(ai_service.unwrap());
    info!("Audio interface service successfully created.");

    let player_service = PlayerService::new(config.clone());
    if let Err(e) = &player_service {
        error!("Player service can't be created. error: {}", e);
        start_degraded(&mut term_signal, config.clone()).await;
    }

    let player_service = Arc::new(Mutex::new(player_service.unwrap()));
    info!("Player service successfully created.");

    let (input_commands_tx, input_commands_rx) = tokio::sync::mpsc::channel(1);

    // start playing after start
    _ = input_commands_tx.send(Command::Play).await;

    let (state_changes_tx, _) = broadcast::channel(20);

    let (http_server_future, websocket_future) = http_api::server_warp::start(
        state_changes_tx.subscribe(),
        input_commands_tx.clone(),
        config.clone(),
        player_service.clone(),
    );

    tokio::select! {
        _ = control::ir_lirc::listen(input_commands_tx.clone()) => {
            error!("Exit from IR Command thread.");
        }

        _ = monitor::oled::write(state_changes_tx.subscribe(), config.clone()) => {
            error!("Exit from OLED writer thread.");
        }

        _ = monitor::status::monitor(
            player_service.clone(),
            state_changes_tx.clone(),
            ai_service.clone(),
        ) => {
            error!("Exit from status monitor thread.");
        }

        _
         = command_handler::handle(
            player_service.clone(),
            ai_service,
            config.clone(),
            input_commands_rx,
            state_changes_tx.clone(),
            state_changes_tx.subscribe(),
        ) => {
            error!("Exit from command handler thread.");
        }

        _ = http_server_future => {}

        _ = websocket_future => {}

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    };

    info!("DPlayer shutdown completed.");
}
async fn start_degraded(term_signal: &mut Signal, config: Arc<Mutex<Configuration>>) {
    warn!("Starting server in degraded mode.");
    let http_server_future = http_api::server_warp::start_degraded(config);
    tokio::select! {
        _ = http_server_future => {}

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }

    }
}
