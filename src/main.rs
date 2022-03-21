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

use crate::control::command_handler::handle;
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

        #[cfg(feature = "hw_ir_control")]
        if settings.ir_control_settings.enabled {
            threads.push(control::ir_lirc::start(
                input_commands_tx.clone(),
                state_changes_sender.subscribe(),
            ));
        }
        #[cfg(feature = "hw_oled")]
        if settings.oled_settings.enabled {
            threads.push(monitor::oled::start(state_changes_sender.subscribe()));
        }

        let player_factory = Arc::new(Mutex::new(player_factory));

        threads.push(spawn(monitor::status::monitor(
            player_factory.clone(),
            state_changes_sender.clone(),
            state_changes_sender.subscribe(),
            audio_card.clone(),
        )));

        // start command handler thread
        threads.push(spawn(handle(
            #[cfg(feature = "hw_dac")]
            dac.clone(),
            player_factory.clone(),
            audio_card.clone(),
            config.clone(),
            input_commands_rx,
            state_changes_sender.clone(),
            state_changes_sender.subscribe(),
        )));

        // send play command to start playing on last used player
        _ = input_commands_tx.send(Command::Play).await;
    } else if let Err(pf_err) = player_factory {
        error!(
            "Configured player {:?} can not be created. Please use settings page to enter correct configuration.\n Error: {:?}",
            current_player, pf_err
        );
    }

    // start http server
    let (http_handle, ws_handle) = http_api::server_warp::start(
        state_changes_sender.subscribe(),
        input_commands_tx.clone(),
        config.clone(),
    );
    threads.push(tokio::task::spawn(http_handle));
    threads.push(ws_handle);

    info!("DPlayer started.");

    signal(SignalKind::terminate()).unwrap().recv().await;
    info!("Gracefull shutdown started");
    _ = state_changes_sender
        .clone()
        .send(api_models::player::StatusChangeEvent::Shutdown);

    for t in threads {
        _ = t.await;
    }

    info!("Gracefull shuttdown completed.");
}
