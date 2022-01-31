//#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate strum_macros;

mod audio_device;
mod common;
mod config;
mod control;
mod http_api;
mod mcu;
mod monitor;
mod player;

use common::{CommandEvent, PlayerType};
use mockall_double::double;
use monitor::status::StatusMonitor;
use std::{
    panic,
    sync::{mpsc, Arc, Mutex},
};
use unix_socket::UnixStream;

#[double]
use crate::audio_device::ak4497::Dac;
#[double]
use crate::audio_device::alsa::AudioCard;
use crate::common::Command;
use crate::common::DPLAY_CONFIG_DIR_PATH;
use crate::player::PlayerFactory;
extern crate env_logger;
#[macro_use]
extern crate log;
use log4rs;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting Dplayer!");

    let mut config = config::Configuration::new();
    let settings = config.get_settings();

    let dac = Arc::new(Dac::new(
        config.get_dac_status(),
        &settings
            .dac_settings
            .as_ref()
            .expect("Dac Integration is not enabled"),
    ));
    let audio_card = Arc::new(AudioCard::new(settings.alsa_settings.device_name.clone()));
    audio_card.wait_unlock_audio_dev();

    panic::set_hook(Box::new(|x| {
        error!("IGNORE PANIC: {}", x);
    }));

    let (input_commands_tx, input_commands_rx) = mpsc::sync_channel(1);
    control::ir_lirc::start(
        input_commands_tx.clone(),
        Arc::new(Mutex::new(
            UnixStream::connect("/var/run/lirc/lircd").unwrap(),
        )),
    );

    let player_factory = Arc::new(Mutex::new(PlayerFactory::new(
        &config.get_streamer_status().source_player,
        settings.clone(),
    )));

    let config = Arc::new(Mutex::new(config));
    let (state_changes_sender, _) = broadcast::channel(20);
    // poll player and dac and produce event if something has changed
    StatusMonitor::start(
        player_factory.clone(),
        state_changes_sender.clone(),
        audio_card.clone(),
    );

    // start http server
    let http_handle = http_api::server_warp::start(
        state_changes_sender.subscribe(),
        input_commands_tx.clone(),
        config.clone(),
    );
    monitor::oled::start(state_changes_sender.subscribe());

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
    state_changes_sender
        .send(CommandEvent::DacStatusChanged(
            config.lock().unwrap().get_dac_status(),
        ))
        .expect("Event send failed");
    state_changes_sender
        .send(CommandEvent::StreamerStatusChanged(
            config.lock().unwrap().get_streamer_status(),
        ))
        .expect("Event send failed");

    http_handle.await;
}
