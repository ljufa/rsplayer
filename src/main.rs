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

use api_models::player::Command;
use monitor::status::StatusMonitor;
use std::sync::{mpsc, Arc, Mutex};
use unix_socket::UnixStream;

use crate::audio_device::ak4497::Dac;

use crate::audio_device::alsa::AudioCard;

use crate::player::PlayerFactory;

use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting Dplayer!");

    let mut config = config::Configuration::new();
    let settings = config.get_settings();

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
    let audio_card = Arc::new(AudioCard::new(settings.alsa_settings.device_name.clone()));
    let card_ok = audio_card.wait_unlock_audio_dev();
    if let Err(err) = card_ok {
        error!("Audio card error: {}", err);
        std::process::exit(1);
    } else {
        info!("Audio card is succesfully initialized.");
    }

    let (input_commands_tx, input_commands_rx) = mpsc::sync_channel(1);
    let (state_changes_sender, _) = broadcast::channel(20);
    let current_player = &config.get_streamer_status().source_player;
    let player_factory = PlayerFactory::new(current_player, settings.clone());
    let config = Arc::new(Mutex::new(config));
    if let Ok(player_factory) = player_factory {
        info!("Player succesfully created.");
        if settings.ir_control_settings.enabled {
            control::ir_lirc::start(
                input_commands_tx.clone(),
                Arc::new(Mutex::new(
                    UnixStream::connect("/var/run/lirc/lircd").unwrap(),
                )),
            );
        }
        if settings.oled_settings.enabled {
            monitor::oled::start(state_changes_sender.subscribe());
        }

        let player_factory = Arc::new(Mutex::new(player_factory));

        // poll player and dac and produce event if something has changed
        StatusMonitor::start(
            player_factory.clone(),
            state_changes_sender.clone(),
            audio_card.clone(),
        );
        // start command handler thread
        control::command_handler::start(
            dac.clone(),
            player_factory.clone(),
            audio_card.clone(),
            config.clone(),
            input_commands_rx,
            state_changes_sender.clone(),
        );

        // send play command to start playing on last used player
        input_commands_tx.send(Command::Play).expect("Error");
    } else if let Err(pf_err) = player_factory {
        error!(
            "Configured player {:?} can not be created. Please use settings page to enter correct configuration.\n Error: {:?}",
            current_player, pf_err
        );
    }

    // start http server
    let http_handle = http_api::server_warp::start(
        state_changes_sender.subscribe(),
        input_commands_tx.clone(),
        config.clone(),
    );
    info!("DPlayer started.");
    http_handle.await;
}
