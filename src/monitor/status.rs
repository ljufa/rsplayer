use crate::player::PlayerFactory;
use crate::{audio_device::alsa::AudioCard, common::CommandEvent};
use std::{ops::Deref, time::Duration};
use std::{
    sync::{Arc, Mutex},
    thread,
};
use tokio::sync::broadcast::Sender;

pub struct StatusMonitor {}
impl StatusMonitor {
    pub fn new() -> Self {
        StatusMonitor {}
    }

    pub fn start(
        player_factory: Arc<Mutex<PlayerFactory>>,
        state_changes_tx: Sender<CommandEvent>,
        audio_card: Arc<AudioCard>,
    ) {
        std::thread::spawn(move || {
            let mut last_status = None;
            loop {
                thread::sleep(Duration::from_millis(500));
                if (audio_card.is_device_in_use()) {
                    let new_status = player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_status();

                    if let (Some(ls), Some(ns)) = (last_status.as_ref(), new_status.as_ref()) {
                        if ls != ns {
                            state_changes_tx
                                .send(CommandEvent::PlayerStatusChanged(ns.clone()))
                                .expect("Send command event failed.");
                        }
                    }
                    last_status = new_status;
                }
            }
        });
    }
}
