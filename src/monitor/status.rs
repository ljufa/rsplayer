use crate::player::PlayerFactory;
use crate::{audio_device::alsa::AudioCard, common::CommandEvent};
use std::time::Duration;
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
            let mut last_track_info = None;
            let mut last_player_info = None;
            loop {
                thread::sleep(Duration::from_millis(1000));
                if audio_card.is_device_in_use() {
                    let new_track_info = player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_current_track_info();
                    if let (Some(last), Some(new)) =
                        (last_track_info.as_ref(), new_track_info.as_ref())
                    {
                        if last != new {
                            state_changes_tx
                                .send(CommandEvent::CurrentTrackInfoChanged(new.clone()))
                                .expect("Send command event failed.");
                        }
                    }
                    last_track_info = new_track_info;

                    let new_player_info = player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_player_info();
                    if let (Some(last_p_info), Some(new_p_info)) =
                        (last_player_info.as_ref(), new_player_info.as_ref())
                    {
                        if last_p_info != new_p_info {
                            state_changes_tx
                                .send(CommandEvent::PlayerInfoChanged(new_p_info.clone()))
                                .expect("Sending command event failed");
                        }
                    }
                    last_player_info = new_player_info;
                }
            }
        });
    }
}
