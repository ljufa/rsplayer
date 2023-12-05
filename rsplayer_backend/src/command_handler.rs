use std::process::exit;
use std::sync::Arc;

use api_models::common::MetadataCommand::{QueryLocalFiles, RescanMetadata};
use api_models::common::PlayerCommand::{
    Next, Pause, Play, PlayItem, Prev, QueryCurrentPlayerInfo, RandomToggle, Rewind,
};
use api_models::common::PlaylistCommand::{
    QueryDynamicPlaylists, QueryPlaylistItems, QuerySavedPlaylist, SaveQueueAsPlaylist,
};
use api_models::common::QueueCommand::{
    self, AddLocalLibDirectory, AddSongToQueue, ClearQueue, LoadAlbumInQueue, LoadPlaylistInQueue,
    LoadSongToQueue, QueryCurrentPlayingContext, QueryCurrentSong, RemoveItem,
};
use api_models::common::SystemCommand::{
    ChangeAudioOutput, PowerOff, QueryCurrentStreamerState, RestartRSPlayer, RestartSystem, SetVol,
    VolDown, VolUp,
};
use api_models::common::UserCommand::{Metadata, Player, Playlist, Queue};
use api_models::common::{SystemCommand, UserCommand};

use api_models::state::StateChangeEvent;

use log::debug;
use rsplayer_config::ArcConfiguration;
use rsplayer_metadata::metadata::MetadataService;

use rsplayer_metadata::playlist::PlaylistService;
use rsplayer_metadata::queue::QueueService;

use rsplayer_playback::rsp::PlayerService;
use tokio::sync::broadcast::Sender;

use rsplayer_hardware::audio_device::audio_service::ArcAudioInterfaceSvc;

#[allow(clippy::too_many_lines)]
pub async fn handle_user_commands(
    player_service: Arc<PlayerService>,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
    queue_service: Arc<QueueService>,
    _config_store: ArcConfiguration,
    mut input_commands_rx: tokio::sync::mpsc::Receiver<UserCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) -> ! {
    loop {
        let Some(cmd) = input_commands_rx.recv().await else {
            continue;
        };
        debug!("Received command {:?}", cmd);
        match cmd {
            /*
             * Player commands
             */
            Player(Play) => {
                player_service.play_from_current_queue_song();
            }
            Player(PlayItem(id)) => {
                player_service.play_song(&id).await;
            }
            Player(Pause) => {
                player_service.pause_current_song();
            }
            Player(Next) => {
                player_service.play_next_song().await;
            }
            Player(Prev) => {
                player_service.play_prev_song().await;
            }
            Player(Rewind(sec)) => {
                player_service.seek_current_song(sec);
            }
            Player(RandomToggle) => player_service.toggle_random_play(),
            Player(QueryCurrentPlayerInfo) => {
                state_changes_sender
                    .send(StateChangeEvent::PlayerInfoEvent(
                        player_service.get_player_info(),
                    ))
                    .unwrap();
            }

            /*
             * Playlist commands
             */
            Playlist(SaveQueueAsPlaylist(playlist_name)) => {
                playlist_service.save_new_playlist(&playlist_name, &queue_service.get_all_songs());
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!(
                        "Playlist {playlist_name} saved."
                    )))
                    .unwrap();
            }
            Playlist(QueryPlaylistItems(playlist_id, page_no)) => {
                state_changes_sender
                    .send(StateChangeEvent::PlaylistItemsEvent(
                        playlist_service.get_dynamic_playlist_items(&playlist_id, page_no),
                        page_no,
                    ))
                    .unwrap();
            }
            Playlist(QueryDynamicPlaylists(category_ids, offset, limit)) => {
                let dynamic_pls =
                    playlist_service.get_dynamic_playlists(category_ids, offset, limit);
                state_changes_sender
                    .send(StateChangeEvent::DynamicPlaylistsPageEvent(dynamic_pls))
                    .unwrap();
            }
            Playlist(QuerySavedPlaylist) => {
                print!("");
            }

            /*
             * Queue commands
             */
            Queue(AddSongToQueue(song_id)) => {
                queue_service.add_song_by_id(&song_id);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "1 song added to queue".to_string(),
                    ))
                    .unwrap();
            }
            Queue(ClearQueue) => {
                player_service.stop_current_song().await;
                queue_service.clear();
            }
            Queue(RemoveItem(song_id)) => {
                queue_service.remove_song(&song_id);
            }
            Queue(LoadPlaylistInQueue(pl_id)) => {
                player_service.stop_current_song().await;
                queue_service.load_playlist_in_queue(&pl_id);
                player_service.play_from_current_queue_song();
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "Playlist loaded into queue".to_string(),
                    ))
                    .unwrap();
            }
            Queue(LoadAlbumInQueue(_album_id)) => {}

            Queue(LoadSongToQueue(song_id)) => {
                if let Some(song) = metadata_service.find_song_by_id(&song_id).as_ref() {
                    player_service.stop_current_song().await;
                    queue_service.clear();
                    queue_service.add_song(song);
                    player_service.play_from_current_queue_song();
                    state_changes_sender
                        .send(StateChangeEvent::NotificationSuccess(
                            "Queue replaced with one song".to_string(),
                        ))
                        .unwrap();
                }
            }
            Queue(QueryCurrentSong) => {
                if let Some(song) = queue_service.get_current_song() {
                    state_changes_sender
                        .send(StateChangeEvent::CurrentSongEvent(song))
                        .unwrap();
                }
            }
            Queue(QueryCurrentPlayingContext(query)) => {
                if let Some(pc) = queue_service.get_current_playing_context(query) {
                    state_changes_sender
                        .send(StateChangeEvent::CurrentPlayingContextEvent(pc))
                        .unwrap();
                }
            }
            Queue(AddLocalLibDirectory(dir)) => {
                queue_service.add_songs_from_dir(&dir);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!(
                        "Dir {dir} added to queue"
                    )))
                    .unwrap();
            }
            Queue(QueueCommand::LoadLocalLibDirectory(dir)) => {
                player_service.stop_current_song().await;
                queue_service.load_songs_from_dir(&dir);
                player_service.play_from_current_queue_song();
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!(
                        "Dir {dir} loaded to queue"
                    )))
                    .unwrap();
            }
            /*
             * Metadata commands
             */
            Metadata(RescanMetadata(_music_dir, full_scan)) => {
                let mtds = metadata_service.clone();
                let state_changes_sender = state_changes_sender.clone();
                std::thread::Builder::new()
                    .name("metadata_scanner".to_string())
                    .spawn(move || mtds.scan_music_dir(full_scan, &state_changes_sender))
                    .expect("Failed to start metadata scanner thread");
            }
            Metadata(QueryLocalFiles(dir, _)) => {
                let items = metadata_service.get_items_by_dir(&dir);
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(items))
                    .unwrap();
            }
        }
    }
}
pub async fn handle_system_commands(
    ai_service: ArcAudioInterfaceSvc,
    _metadata_service: Arc<MetadataService>,
    config_store: ArcConfiguration,
    mut input_commands_rx: tokio::sync::mpsc::Receiver<SystemCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    loop {
        if let Some(cmd) = input_commands_rx.recv().await {
            debug!("Received command {:?}", cmd);
            match cmd {
                SetVol(val) => {
                    let nv = ai_service.set_volume(i64::from(val));
                    let new_state = config_store.save_volume_state(nv);
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(new_state))
                        .expect("Send event failed.");
                }
                VolUp => {
                    let nv = ai_service.volume_up();
                    let new_state = config_store.save_volume_state(nv);
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(new_state))
                        .expect("Send event failed.");
                }
                VolDown => {
                    let nv = ai_service.volume_down();
                    let new_state = config_store.save_volume_state(nv);
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(new_state))
                        .expect("Send event failed.");
                }
                ChangeAudioOutput => {
                    if let Some(out) = ai_service.toggle_output() {
                        let new_state = config_store.save_audio_output(out);
                        state_changes_sender
                            .send(StateChangeEvent::StreamerStateEvent(new_state))
                            .unwrap();
                    };
                }
                PowerOff => {
                    std::process::Command::new("systemctl")
                        .arg("poweroff")
                        .spawn()
                        .expect("halt command failed");
                    exit(0);
                }
                RestartSystem => {
                    std::process::Command::new("systemctl")
                        .arg("reboot")
                        .spawn()
                        .expect("halt command failed");
                    exit(0);
                }
                RestartRSPlayer => {
                    let rs = std::process::Command::new("systemctl")
                        .arg("restart")
                        .arg("rsplayer")
                        .spawn();
                    if rs.is_err() {
                        exit(1)
                    }
                }
                QueryCurrentStreamerState => {
                    let ss = config_store.get_streamer_state();
                    state_changes_sender
                        .send(StateChangeEvent::StreamerStateEvent(ss))
                        .unwrap();
                }
            }
        }
    }
}
