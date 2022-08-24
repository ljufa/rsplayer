use std::time::Duration;

use api_models::common::Command;
use api_models::common::Command::*;
use api_models::state::StateChangeEvent;
use cfg_if::cfg_if;

use tokio::sync::broadcast::Sender;

use crate::common::{ArcAudioInterfaceSvc, MutArcConfiguration, MutArcPlayerService};

cfg_if! {
if #[cfg(feature = "hw_gpio")] {
    use crate::mcu::gpio;
    use crate::mcu::gpio::GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY;
    use api_models::state::AudioOut;
}}

pub async fn handle(
    player_service: MutArcPlayerService,
    ai_service: ArcAudioInterfaceSvc,
    config_store: MutArcConfiguration,
    mut input_commands_rx: tokio::sync::mpsc::Receiver<Command>,
    state_changes_sender: Sender<StateChangeEvent>,
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
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(cmd) = input_commands_rx.try_recv() {
            trace!("Received command {:?}", cmd);
            match cmd {
                SetVol(val) => {
                    if let Ok(nv) = ai_service.set_volume(val as i64) {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateEvent(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                VolUp => {
                    if let Ok(nv) = ai_service.volume_up() {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateEvent(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                VolDown => {
                    if let Ok(nv) = ai_service.volume_down() {
                        let new_dac_status =
                            config_store.lock().unwrap().save_volume_state(nv.current);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateEvent(new_dac_status))
                            .expect("Send event failed.");
                    }
                }
                // player commands
                Play => {
                    player_service.lock().unwrap().get_current_player().play();
                }
                PlayItem(id) => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .play_item(id);
                }
                RemovePlaylistItem(id) => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .remove_playlist_item(id);
                }
                Pause => {
                    player_service.lock().unwrap().get_current_player().pause();
                }
                Next => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .next_track();
                }
                Prev => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .prev_track();
                }
                Rewind(sec) => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .rewind(sec);
                }
                LoadPlaylist(pl_id) => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .load_playlist(pl_id);
                }
                LoadAlbum(album_id) => {
                    player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .load_album(album_id);
                }
                LoadSong(song_id) => {
                    
                }
                AddSongToQueue(song_id) => {

                }

                // system commands
                #[cfg(feature = "hw_gpio")]
                ChangeAudioOutput => {
                    let out = if out_sel_pin.get_value().unwrap() == 0 {
                        let _ = out_sel_pin.set_value(1).is_ok();
                        AudioOut::HEAD
                    } else {
                        let _ = out_sel_pin.set_value(0);
                        AudioOut::SPKR
                    };
                    let new_state = config_store.lock().unwrap().save_audio_output(out);
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(new_state))
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
                /*
                 * Query commands
                 */
                QueryCurrentSong => {
                    if let Some(song) = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_current_song()
                    {
                        state_changes_sender
                            .send(StateChangeEvent::CurrentSongEvent(song))
                            .unwrap();
                    }
                }
                QueryCurrentPlayingContext(query) => {
                    if let Some(pc) = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_playing_context(query)
                    {
                        state_changes_sender
                            .send(StateChangeEvent::CurrentPlayingContextEvent(pc))
                            .unwrap();
                    }
                }
                QueryCurrentPlayerInfo => {
                    if let Some(pi) = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_player_info()
                    {
                        state_changes_sender
                            .send(StateChangeEvent::PlayerInfoEvent(pi))
                            .unwrap();
                    }
                }
                QueryCurrentStreamerState => {
                    let ss = config_store.lock().unwrap().get_streamer_status();
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(ss))
                        .unwrap();
                }
                QueryDynamicPlaylists(category_ids, offset, limit) => {
                    let dynamic_pls = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_dynamic_playlists(category_ids, offset, limit);
                    state_changes_sender
                        .send(StateChangeEvent::DynamicPlaylistsPageEvent(dynamic_pls))
                        .unwrap();
                }
                QueryPlaylistItems(playlist_id) => {
                    let pl_items = player_service
                        .lock()
                        .unwrap()
                        .get_current_player()
                        .get_playlist_items(playlist_id);
                    state_changes_sender
                        .send(StateChangeEvent::PlaylistItemsEvent(pl_items))
                        .unwrap();
                }
                

                _ => {}
            }
        }
    }
}
