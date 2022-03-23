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

use crate::control::command_handler::{self, handle};
use api_models::player::Command;
use cfg_if::cfg_if;

use std::sync::{Arc, Mutex};
use tokio::signal::unix::{signal, SignalKind};
use tokio::spawn;
use tokio::task::JoinHandle;

use crate::audio_device::alsa::AudioCard;

use crate::player::PlayerFactory;

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
                let dac = Dac::new(
                config.get_streamer_status().dac_status,
                &settings.dac_settings,
            );
            if let Err(dac_err) = dac {
                error!("DAC initialization error: {}", dac_err);
                std::process::exit(1);
            } else {
                info!("Dac is successfully initialized.");
            }
            let dac = Arc::new(dac.unwrap());
        }
    }
    let audio_card = Arc::new(AudioCard::new(settings.alsa_settings.device_name.clone()));

    let (input_commands_tx, input_commands_rx) = tokio::sync::mpsc::channel(1);
    let (state_changes_sender, _) = broadcast::channel(20);
    let current_player = &config.get_streamer_status().source_player;
    let player_factory = PlayerFactory::new(current_player, settings.clone());
    let config = Arc::new(Mutex::new(config));

    let mut threads: Vec<JoinHandle<()>> = vec![];

    if let Ok(player_factory) = player_factory {
        info!("Player succesfully created.");

        let player_factory = Arc::new(Mutex::new(player_factory));

        let (http_handle, ws_handle) = http_api::server_warp::start(
            state_changes_sender.subscribe(),
            input_commands_tx.clone(),
            config.clone(),
        );
        let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate())
            .expect("failed to create signal future");

        _ = input_commands_tx.send(Command::Play).await;

        tokio::select! {
            _ = #[cfg(feature="hw_ir_control")] control::ir_lirc::listen(input_commands_tx.clone()) => { error!("Exit from IR Command thread."); }
            _ = #[cfg(feature="hw_oled")] monitor::oled::write(state_changes_sender.subscribe()) => {error!("Exit from OLED writer thread.");}
            _ = monitor::status::monitor(
                player_factory.clone(),
                state_changes_sender.clone(),
                audio_card.clone(),
            ) => {error!("Exit from status monitor thread.");}
            _ = command_handler::handle(
                #[cfg(feature = "hw_dac")]
                dac.clone(),
                player_factory.clone(),
                audio_card,
                config.clone(),
                input_commands_rx,
                state_changes_sender.clone(),
                state_changes_sender.subscribe(),
            ) => {}
            _ = http_handle => {}
            _ = ws_handle => {}
            _ = term_signal.recv() => {
                info!("Terminate signal received.");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("CTRL-c signal received.");
            }
        };
    } else if let Err(pf_err) = player_factory {
        error!(
            "Configured player {:?} can not be created. Please use settings page to enter correct configuration.\n Error: {:?}",
            current_player, pf_err
        );
    }
    info!("Gracefull shuttdown completed.");
}
