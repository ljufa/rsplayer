cfg_if! {
if #[cfg(feature = "hw_gpio")] {
    use crate::mcu::gpio;
    use crate::mcu::gpio::GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY;

}}
#[cfg(feature = "hw_dac")]
use crate::audio_device::ak4497::Dac;

use crate::audio_device::alsa::AudioCard;
use crate::config::Configuration;
use crate::player::PlayerFactory;

use std::sync::{Arc, Mutex};

use api_models::player::Command::*;

use api_models::player::*;
use cfg_if::cfg_if;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::Sender;

pub async fn handle(
    #[cfg(feature = "hw_dac")] dac: Arc<Dac>,
    player_factory: Arc<Mutex<PlayerFactory>>,
    audio_card: Arc<AudioCard>,
    config_store: Arc<Mutex<Configuration>>,
    mut input_commands_rx: tokio::sync::mpsc::Receiver<Command>,
    state_changes_sender: Sender<StatusChangeEvent>,
    mut state_changes_receiver: Receiver<StatusChangeEvent>,
) {
    //fixme : move to separate struct restore selected output
    cfg_if! {
    if #[cfg(feature = "hw_gpio")] {
        let out_sel_pin = gpio::get_output_pin_handle(GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY);
        let _i = match config_store
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
        if let Ok(StatusChangeEvent::Shutdown) = state_changes_receiver.try_recv() {
            info!("Stop command handler.");
            break;
        }
        if let Ok(cmd) = input_commands_rx.try_recv() {
            trace!("Received command {:?}", cmd);
            match cmd {
                // dac commands
                #[cfg(feature = "hw_dac")]
                SetVol(val) => {
                    if let Ok(nv) = dac.set_vol(val) {
                        let new_dac_status =
                            config_store
                                .lock()
                                .unwrap()
                                .patch_dac_status(Some(nv), None, None);
                        state_changes_sender
                            .send(StatusChangeEvent::StreamerStatusChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                #[cfg(feature = "hw_dac")]
                VolUp => {
                    if let Ok(nv) = dac.vol_up() {
                        let new_dac_status =
                            config_store
                                .lock()
                                .unwrap()
                                .patch_dac_status(Some(nv), None, None);
                        state_changes_sender
                            .send(StatusChangeEvent::StreamerStatusChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                #[cfg(feature = "hw_dac")]
                VolDown => {
                    if let Ok(nv) = dac.vol_down() {
                        let new_dac_status =
                            config_store
                                .lock()
                                .unwrap()
                                .patch_dac_status(Some(nv), None, None);
                        state_changes_sender
                            .send(StatusChangeEvent::StreamerStatusChanged(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                #[cfg(feature = "hw_dac")]
                Filter(ft) => {
                    if let Ok(nv) = dac.filter(ft) {
                        let new_streamer_status =
                            config_store
                                .lock()
                                .unwrap()
                                .patch_dac_status(None, Some(nv), None);
                        state_changes_sender
                            .send(StatusChangeEvent::StreamerStatusChanged(
                                new_streamer_status,
                            ))
                            .expect("Send event failed.");
                    }
                }
                #[cfg(feature = "hw_dac")]
                Sound(nr) => {
                    if let Ok(nv) = dac.change_sound_setting(nr) {
                        config_store
                            .lock()
                            .unwrap()
                            .patch_dac_status(None, None, Some(nv));
                    }
                }
                #[cfg(feature = "hw_dac")]
                Gain(level) => {
                    dac.set_gain(level);
                }
                #[cfg(feature = "hw_dac")]
                Hload(flag) => {
                    dac.hi_load(flag);
                }
                #[cfg(feature = "hw_dac")]
                Dsd(flag) => {
                    dac.dsd_pcm(flag);
                }
                // player commands
                Play => {
                    player_factory.lock().unwrap().get_current_player().play();
                }
                Pause => {
                    player_factory.lock().unwrap().get_current_player().pause();
                }
                Next => {
                    player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .next_track();
                }
                Prev => {
                    player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .prev_track();
                }
                Rewind(sec) => {
                    player_factory
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .rewind(sec);
                }
                // system commands
                SwitchToPlayer(pt) => {
                    trace!("Switching to player {:?}", pt);
                    let mut cfg = config_store.lock().unwrap();
                    match player_factory
                        .lock()
                        .unwrap()
                        .switch_to_player(audio_card.clone(), &pt)
                    {
                        Ok(npt) => {
                            let new_sstate = cfg.patch_streamer_status(Some(npt), None);
                            state_changes_sender
                                .send(StatusChangeEvent::StreamerStatusChanged(new_sstate))
                                .unwrap();
                        }
                        Err(_e) => {
                            state_changes_sender
                                .send(StatusChangeEvent::Error(String::from("Change failed!")))
                                .unwrap();
                        }
                    }
                }
                #[cfg(feature = "hw_gpio")]
                ChangeAudioOutput => {
                    let nout;
                    if out_sel_pin.get_value().unwrap() == 0 {
                        out_sel_pin.set_value(1);
                        nout = AudioOut::HEAD;
                    } else {
                        out_sel_pin.set_value(0);
                        nout = AudioOut::SPKR;
                    }
                    let new_sstate = config_store
                        .lock()
                        .unwrap()
                        .patch_streamer_status(None, Some(nout));
                    state_changes_sender
                        .send(StatusChangeEvent::StreamerStatusChanged(new_sstate))
                        .unwrap();
                }
                PowerOff => {
                    std::process::Command::new("/sbin/poweroff")
                        .spawn()
                        .expect("halt command failed");
                }
                RandomToggle => player_factory
                    .lock()
                    .unwrap()
                    .get_current_player()
                    .random_toggle(),
                _ => {}
            }
        }
    }
}
