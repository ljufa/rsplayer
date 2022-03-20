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
use futures::FutureExt;
use monitor::status::StatusMonitor;
use std::sync::{mpsc, Arc, Mutex};
use tokio::signal::unix::{signal, Signal, SignalKind};
use tokio::task::JoinHandle;
use unix_socket::UnixStream;
use warp::ws;

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
    
    #[cfg(target_arch = "aarch64")]
    let dac = Dac::new(
        config.get_streamer_status().dac_status,
        &settings.dac_settings,
    );
    #[cfg(target_arch = "aarch64")]
    if let Err(dac_err) = dac {
        error!("DAC initialization error: {}", dac_err);
        std::process::exit(1);
    } else {
        info!("Dac is successfully initialized.");
    }
    
    #[cfg(target_arch = "aarch64")]
    let dac = Arc::new(dac.unwrap());
    let audio_card = Arc::new(AudioCard::new(settings.alsa_settings.device_name.clone()));

    let (input_commands_tx, input_commands_rx) = mpsc::sync_channel(1);
    let (state_changes_sender, _) = broadcast::channel(20);
    let current_player = &config.get_streamer_status().source_player;
    let player_factory = PlayerFactory::new(current_player, settings.clone());
    let config = Arc::new(Mutex::new(config));

    let mut threads: Vec<JoinHandle<()>> = vec![];
    if let Ok(player_factory) = player_factory {
        info!("Player succesfully created.");
        let player_factory = Arc::new(Mutex::new(player_factory));
        
        #[cfg(target_arch = "aarch64")]
        if settings.ir_control_settings.enabled {
            threads.push(control::ir_lirc::start(
                input_commands_tx.clone(),
                Arc::new(Mutex::new(
                    UnixStream::connect("/var/run/lirc/lircd").unwrap(),
                )),
            ));
        }
        #[cfg(target_arch = "aarch64")]
        if settings.oled_settings.enabled {
            threads.push(monitor::oled::start(state_changes_sender.subscribe()));
        }
    
        // poll player and dac and produce event if something has changed
        threads.push(StatusMonitor::start(
            player_factory.clone(),
            state_changes_sender.clone(),
            audio_card.clone(),
        ));
        // start command handler thread
        threads.push(control::command_handler::start(
            #[cfg(target_arch="aarch64")]
            dac.clone(),
            player_factory.clone(),
            audio_card.clone(),
            config.clone(),
            input_commands_rx,
            state_changes_sender.clone(),
        ));

        // send play command to start playing on last used player
        input_commands_tx.send(Command::Play).expect("Error");
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
    tokio::task::spawn(http_handle);
    info!("Http server started.");

    threads.push(ws_handle);

    info!("DPlayer started.");

    signal(SignalKind::terminate()).unwrap().recv().await;

    info!("Gracefull shutdown started");

    for t in &threads {
        debug!("Aborting thread {:?}", t);
        t.abort();
    }

    info!("Gracefull shuttdown completed.");
}
