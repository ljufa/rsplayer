cfg_if! {
if #[cfg(feature = "hw_gpio")] {
    use crate::mcu::gpio;
    use crate::mcu::gpio::GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY;
    use api_models::state::AudioOut;
}}

use crate::common::{ArcAudioInterfaceSvc, MutArcConfiguration, MutArcPlayerService};

use std::time::Duration;

use api_models::common::Command::*;

use api_models::common::Command;
use api_models::state::StateChangeEvent;
use cfg_if::cfg_if;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::Sender;

pub async fn handle(
    player_service: MutArcPlayerService,
    ai_service: ArcAudioInterfaceSvc,
    config_store: MutArcConfiguration,
    mut input_commands_rx: tokio::sync::mpsc::Receiver<Command>,
    state_changes_sender: Sender<StateChangeEvent>,
    mut state_changes_receiver: Receiver<StateChangeEvent>,
) {
    //fixme : move to separate struct restore selected output
    cfg_if! {
    if #[cfg(feature = "hw_gpio")] {
        let out_sel_pin = gpio::get_output_pin_handle(GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY);
        let _ = match config_store
        .lock()
        .unwrap()
        .get_streamer_status()
        .selected_audio_output
    {
        AudioOut::SPKR => out_sel_pin.set_value(0),
        AudioOut::HEAD => out_sel_pin.set_value(1),
    };

    }}

    loop {
        if let Ok(StateChangeEvent::Shutdown) = state_changes_receiver.try_recv() {
            info!("Stop command handler.");
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(cmd) = input_commands_rx.try_recv() {
            trace!("Received command {:?}", cmd);
            match cmd {
                SetVol(val) => {
                    if let Ok(nv) = ai_service.set_volume(val) {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                VolUp => {
                    if let Ok(nv) = ai_service.volume_up() {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                VolDown => {
                    if let Ok(nv) = ai_service.volume_down() {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                // player commands
                Play => {
                    _ = player_service.lock().unwrap().get_current_player().play();
                }
                PlayAt(position) => {
                    _ = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .play_at(position);
                }
                Pause => {
                    _ = player_service.lock().unwrap().get_current_player().pause();
                }
                Next => {
                    _ = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .next_track();
                }
                Prev => {
                    _ = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .prev_track();
                }
                Rewind(sec) => {
                    _ = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .rewind(sec);
                }
                LoadPlaylist(pl_id) => {
                    _ = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .load_playlist(pl_id);
                }
                // system commands
                #[cfg(feature = "hw_gpio")]
                ChangeAudioOutput => {
                    let nout = if out_sel_pin.get_value().unwrap() == 0 {
                        _ = out_sel_pin.set_value(1).is_ok();
                        AudioOut::HEAD
                    } else {
                        _ = out_sel_pin.set_value(0);
                        AudioOut::SPKR
                    };
                    let new_sstate = config_store.lock().unwrap().save_audio_output(nout);
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateChanged(new_sstate))
                        .unwrap();
                }
                PowerOff => {
                    std::process::Command::new("/sbin/poweroff")
                        .spawn()
                        .expect("halt command failed");
                }
                RandomToggle => player_service
                    .lock()
                    .unwrap()
                    .get_current_player()
                    .random_toggle(),
                _ => {}
            }
        }
    }
}
