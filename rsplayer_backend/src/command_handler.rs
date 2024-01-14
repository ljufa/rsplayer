use std::process::exit;
use std::sync::Arc;

use log::debug;
use tokio::sync::broadcast::Sender;

use api_models::common::MetadataCommand::{QueryLocalFiles, RescanMetadata};
use api_models::common::PlayerCommand::{
    Next, Pause, Play, PlayItem, Prev, QueryCurrentPlayerInfo, RandomToggle, Rewind,
};
use api_models::common::PlaylistCommand::{QueryPlaylist, QueryPlaylistItems, SaveQueueAsPlaylist};
use api_models::common::QueueCommand::{
    self, AddLocalLibDirectory, AddSongToQueue, ClearQueue, LoadAlbumInQueue, LoadArtistInQueue, LoadPlaylistInQueue,
    LoadSongToQueue, QueryCurrentPlayingContext, QueryCurrentSong, RemoveItem,
};
use api_models::common::SystemCommand::{
    ChangeAudioOutput, PowerOff, QueryCurrentStreamerState, RestartRSPlayer, RestartSystem, SetVol, VolDown, VolUp,
};
use api_models::common::UserCommand::{Metadata, Player, Playlist, Queue};
use api_models::common::{MetadataCommand, MetadataLibraryItem, MetadataLibraryResult, SystemCommand, UserCommand};
use api_models::playlist::PlaylistType;
use api_models::state::StateChangeEvent;
use rsplayer_config::ArcConfiguration;
use rsplayer_hardware::audio_device::audio_service::ArcAudioInterfaceSvc;
use rsplayer_metadata::album_repository::AlbumRepository;
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::playlist_service::PlaylistService;
use rsplayer_metadata::queue_service::QueueService;
use rsplayer_metadata::song_repository::SongRepository;
use rsplayer_playback::rsp::PlayerService;

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn handle_user_commands(
    player_service: Arc<PlayerService>,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
    queue_service: Arc<QueueService>,
    album_repository: Arc<AlbumRepository>,
    song_repository: Arc<SongRepository>,
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
                    .send(StateChangeEvent::PlayerInfoEvent(player_service.get_player_info()))
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
                let songs = playlist_service
                    .get_playlist_page_by_name(&playlist_id, page_no * 20, 20)
                    .items;
                state_changes_sender
                    .send(StateChangeEvent::PlaylistItemsEvent(songs, page_no))
                    .unwrap();
            }
            Playlist(api_models::common::PlaylistCommand::QueryAlbumItems(album_title, page_no)) => {
                let songs = album_repository.find_by_id(&album_title).map(|alb| alb.song_keys);

                if let Some(songs) = songs {
                    let songs = songs
                        .iter()
                        .skip(page_no * 20)
                        .take(20)
                        .filter_map(|song_key| song_repository.find_by_id(song_key))
                        .collect::<Vec<_>>();
                    state_changes_sender
                        .send(StateChangeEvent::PlaylistItemsEvent(songs, page_no))
                        .unwrap();
                }
            }
            Playlist(QueryPlaylist) => {
                let mut pls = playlist_service.get_playlists();
                album_repository
                    .find_all_sort_by_added_desc(30)
                    .into_iter()
                    .for_each(|alb| {
                        pls.items.push(PlaylistType::RecentlyAdded(alb));
                    });
                album_repository
                    .find_all_sort_by_released_desc(30)
                    .into_iter()
                    .for_each(|alb| {
                        pls.items.push(PlaylistType::LatestRelease(alb));
                    });
                state_changes_sender
                    .send(StateChangeEvent::PlaylistsEvent(pls))
                    .unwrap();
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
                let pl_songs = playlist_service.get_playlist_page_by_name(&pl_id, 0, 20000).items;
                queue_service.replace_all(pl_songs.into_iter());
                player_service.play_from_current_queue_song();
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "Playlist loaded into queue".to_string(),
                    ))
                    .unwrap();
            }
            Queue(QueueCommand::AddPlaylistToQueue(pl_id)) => {
                let pl_songs = playlist_service.get_playlist_page_by_name(&pl_id, 0, 20000).items;
                for song in &pl_songs {
                    queue_service.add_song(song);
                }
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "Playlist added to queue".to_string(),
                    ))
                    .unwrap();
            }
            Queue(LoadAlbumInQueue(album_id)) => {
                if let Some(album) = album_repository.find_by_id(&album_id) {
                    player_service.stop_current_song().await;
                    let songs = album.song_keys.iter().filter_map(|sk| song_repository.find_by_id(sk));
                    queue_service.replace_all(songs);
                    player_service.play_from_current_queue_song();
                    state_changes_sender
                        .send(StateChangeEvent::NotificationSuccess(
                            "Album loaded into queue".to_string(),
                        ))
                        .unwrap();
                };
            }
            Queue(LoadArtistInQueue(name)) => {
                player_service.stop_current_song().await;
                queue_service.clear();
                album_repository
                    .find_by_artist(&name)
                    .iter()
                    .flat_map(|alb| &alb.song_keys)
                    .for_each(|sk| {
                        queue_service.add_song_by_id(sk);
                    });
                player_service.play_from_current_queue_song();
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "All artist's albums loaded into queue".to_string(),
                    ))
                    .unwrap();
            }

            Queue(QueueCommand::AddAlbumToQueue(album_id)) => {
                if let Some(album) = album_repository.find_by_id(&album_id) {
                    album.song_keys.iter().for_each(|sk| {
                        if let Some(song) = song_repository.find_by_id(sk) {
                            queue_service.add_song(&song);
                        }
                    });
                    state_changes_sender
                        .send(StateChangeEvent::NotificationSuccess(
                            "Album added to queue".to_string(),
                        ))
                        .unwrap();
                };
            }
            Queue(QueueCommand::AddArtistToQueue(name)) => {
                album_repository.find_by_artist(&name).iter().for_each(|alb| {
                    alb.song_keys.iter().for_each(|sk| {
                        if let Some(song) = song_repository.find_by_id(sk) {
                            queue_service.add_song(&song);
                        }
                    });
                });
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "All artist's albums added to queue".to_string(),
                    ))
                    .unwrap();
            }

            Queue(LoadSongToQueue(song_id)) => {
                player_service.stop_current_song().await;
                queue_service.clear();
                queue_service.add_song_by_id(&song_id);
                player_service.play_from_current_queue_song();
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "Queue replaced with one song".to_string(),
                    ))
                    .unwrap();
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
                let items = song_repository.find_by_dir(&dir);
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(items))
                    .unwrap();
            }
            Metadata(MetadataCommand::QueryArtists) => {
                let items: Vec<MetadataLibraryItem> = album_repository
                    .find_all_album_artists()
                    .iter()
                    .map(|art| MetadataLibraryItem::Artist { name: art.to_owned() })
                    .collect();
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(MetadataLibraryResult {
                        items,
                        root_path: String::new(),
                    }))
                    .unwrap();
            }
            Metadata(MetadataCommand::QueryAlbumsByArtist(artist)) => {
                let items: Vec<MetadataLibraryItem> = album_repository
                    .find_by_artist(&artist)
                    .iter()
                    .map(|alb| MetadataLibraryItem::Album {
                        name: alb.title.clone(),
                        year: alb.released,
                    })
                    .collect();
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(MetadataLibraryResult {
                        items,
                        root_path: String::new(),
                    }))
                    .unwrap();
            }
            Metadata(MetadataCommand::QuerySongsByAlbum(album)) => {
                let items: Vec<MetadataLibraryItem> = album_repository
                    .find_by_id(&album)
                    .iter()
                    .flat_map(|alb| alb.song_keys.iter().filter_map(|sk| song_repository.find_by_id(sk)))
                    .map(MetadataLibraryItem::SongItem)
                    .collect();
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(MetadataLibraryResult {
                        items,
                        root_path: String::new(),
                    }))
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
