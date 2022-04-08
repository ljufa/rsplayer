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

use crate::control::command_handler;

use api_models::player::Command;
use cfg_if::cfg_if;

use std::panic;
use std::sync::{Arc, Mutex};
use tokio::signal::unix::SignalKind;

use crate::audio_device::alsa::AudioCard;

use crate::player::PlayerService;

use tokio::sync::broadcast;

#[cfg(feature = "hw_dac")]
use crate::audio_device::ak4497::Dac;

#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Starting Dplayer!");

    let mut config = config::Configuration::new();

    let settings = config.get_settings();

    cfg_if! {
            if #[cfg(feature = "hw_dac")] {
                let dac = Dac::new(config.get_streamer_status().dac_status, &settings.dac_settings);
                if let Err(dac_err) = dac {
                    error!("DAC initialization error: {}", dac_err);
                    std::process::exit(1);
                } else {
                    info!("Dac is successfully initialized.");
                }
                let dac = Arc::new(dac.unwrap());
            }
    }

    let current_player = &config.get_settings().active_player;

    let config = Arc::new(Mutex::new(config));

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to create signal future");

    match PlayerService::new(current_player, settings.clone()) {
        Ok(player_service) => {
            info!("Player successfully created.");
            let player_service = Arc::new(Mutex::new(player_service));

            let (input_commands_tx, input_commands_rx) = tokio::sync::mpsc::channel(1);

            // start playing after start
            _ = input_commands_tx.send(Command::Play).await;

            let (state_changes_tx, _) = broadcast::channel(20);

            let audio_card = Arc::new(AudioCard::new(settings.alsa_settings.device_name.clone()));

            let (http_server_future, websocket_future) = http_api::server_warp::start(
                state_changes_tx.subscribe(),
                input_commands_tx.clone(),
                config.clone(),
                player_service.clone(),
                #[cfg(feature = "hw_dac")]
                dac.clone(),
            );

            tokio::select! {
                _ = control::ir_lirc::listen(input_commands_tx.clone()) => {
                    error!("Exit from IR Command thread.");
                }

                _ = monitor::oled::write(state_changes_tx.subscribe()) => {
                    error!("Exit from OLED writer thread.");
                }

                _ = monitor::status::monitor(
                    player_service.clone(),
                    state_changes_tx.clone(),
                    audio_card.clone(),
                ) => {
                    error!("Exit from status monitor thread.");
                }

                _
                 = command_handler::handle(
                    #[cfg(feature = "hw_dac")]
                    dac.clone(),
                    player_service.clone(),
                    audio_card,
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
        }
        Err(err) => {
            error!("Configured player {:?} can not be created. Please use settings page to enter correct configuration. Error: {}", current_player, err);
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
    }
    info!("DPlayer shutdown completed.");
}
