use std::process::exit;
use std::sync::Arc;

use log::debug;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::Receiver;

use api_models::common::MetadataCommand::{QueryLocalFiles, RescanMetadata};
use api_models::common::PlayerCommand::{
    Next, Pause, Play, PlayItem, Prev, QueryCurrentPlayerInfo, RandomToggle, Seek,
};
use api_models::common::PlaylistCommand::{QueryAlbumItems, QueryPlaylist, QueryPlaylistItems, SaveQueueAsPlaylist};
use api_models::common::QueueCommand::{
    self, AddLocalLibDirectory, AddSongToQueue, ClearQueue, LoadAlbumInQueue, LoadArtistInQueue, LoadPlaylistInQueue,
    LoadSongToQueue, QueryCurrentQueue, QueryCurrentSong, RemoveItem,
};
use api_models::common::SystemCommand::{
    ChangeAudioOutput, PowerOff, QueryCurrentStreamerState, RestartRSPlayer, RestartSystem, SetVol, VolDown, VolUp,
};
use api_models::common::UserCommand::{Metadata, Player, Playlist, Queue};
use api_models::common::{MetadataCommand, MetadataLibraryItem, SystemCommand, UserCommand};
use api_models::playlist::PlaylistType;
use api_models::state::StateChangeEvent;
use rsplayer_config::ArcConfiguration;
use rsplayer_hardware::audio_device::audio_service::ArcAudioInterfaceSvc;
use rsplayer_metadata::album_repository::AlbumRepository;
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::playlist_service::PlaylistService;
use rsplayer_metadata::queue_service::QueueService;
use rsplayer_metadata::song_repository::SongRepository;
use rsplayer_playback::rsp::player_service::PlayerService;

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn handle_user_commands(
    player_service: Arc<PlayerService>,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
    queue_service: Arc<QueueService>,
    album_repository: Arc<AlbumRepository>,
    song_repository: Arc<SongRepository>,
    _config_store: ArcConfiguration,
    mut input_commands_rx: Receiver<UserCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    loop {
        let Some(cmd) = input_commands_rx.recv().await else {
            debug!("Wait in loop");
            continue;
        };
        debug!("Received command {:?}", cmd);
        let sender = &state_changes_sender.clone();
        match cmd {
            /*
             * Player commands
             */
            Player(Play) => {
                player_service.play_from_current_queue_song();
                debug!("Play from current song command processed");
            }
            Player(PlayItem(id)) => {
                player_service.play_song(&id);
            }
            Player(Pause) => {
                player_service.pause_current_song();
                sender
                    .send(StateChangeEvent::PlaybackStateEvent(
                        api_models::state::PlayerState::PAUSED,
                    ))
                    .unwrap();
            }
            Player(Next) => {
                player_service.play_next_song();
            }
            Player(Prev) => {
                player_service.play_prev_song();
            }
            Player(api_models::common::PlayerCommand::Stop) => {
                player_service.stop_current_song();
            }
            Player(Seek(sec)) => {
                player_service.seek_current_song(sec);
            }
            Player(RandomToggle) => {
                sender
                    .send(StateChangeEvent::RandomToggleEvent(queue_service.toggle_random_next()))
                    .unwrap();
            }
            Player(QueryCurrentPlayerInfo) => {
                let is_random = queue_service.get_random_next();
                state_changes_sender
                    .send(StateChangeEvent::RandomToggleEvent(is_random))
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
            Playlist(QueryAlbumItems(album_title, page_no)) => {
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
                player_service.stop_current_song();
                queue_service.clear();
            }
            Queue(RemoveItem(song_id)) => {
                queue_service.remove_song(&song_id);
            }
            Queue(LoadPlaylistInQueue(pl_id)) => {
                player_service.stop_current_song();
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
                    player_service.stop_current_song();
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
                player_service.stop_current_song();
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
                player_service.stop_current_song();
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
            Queue(QueryCurrentQueue(query)) => {
                let queue = queue_service.query_current_queue(query);
                state_changes_sender
                    .send(StateChangeEvent::CurrentQueueEvent(queue))
                    .unwrap();
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
                player_service.stop_current_song();
                queue_service.load_songs_from_dir(&dir);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!(
                        "Dir {dir} loaded to queue"
                    )))
                    .unwrap();
                player_service.play_from_current_queue_song();
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
                let items = metadata_service.search_local_files_by_dir(&dir);
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(items))
                    .unwrap();
            }
            Metadata(MetadataCommand::SearchLocalFiles(term, limit)) => {
                let items = metadata_service.search_local_files_by_dir_contains(&term, limit);
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
                    .send(StateChangeEvent::MetadataLocalItems(items))
                    .unwrap();
            }
            Metadata(MetadataCommand::SearchArtists(term)) => {
                let items: Vec<MetadataLibraryItem> = album_repository
                    .find_all_album_artists()
                    .iter()
                    .filter_map(|art| {
                        if art.to_lowercase().contains(&term.to_lowercase()) {
                            Some(MetadataLibraryItem::Artist { name: art.to_owned() })
                        } else {
                            None
                        }
                    })
                    .collect();
                state_changes_sender
                    .send(StateChangeEvent::MetadataLocalItems(items))
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
                    .send(StateChangeEvent::MetadataLocalItems(items))
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
                    .send(StateChangeEvent::MetadataLocalItems(items))
                    .unwrap();
            }
            Metadata(MetadataCommand::LikeMediaItem(id)) => {
                metadata_service.like_media_item(&id);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!("Song {id} liked",)))
                    .unwrap();
            }
            Metadata(MetadataCommand::DislikeMediaItem(id)) => {
                metadata_service.dislike_media_item(&id);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(format!("Song {id} disliked",)))
                    .unwrap();
            }
            Metadata(MetadataCommand::QueryFavoriteRadioStations) => {
                let favorites = metadata_service.get_favorite_radio_stations();
                state_changes_sender
                    .send(StateChangeEvent::FavoriteRadioStations(favorites))
                    .unwrap();
            }
        }
    }
}

pub async fn handle_system_commands(
    ai_service: ArcAudioInterfaceSvc,
    _metadata_service: Arc<MetadataService>,
    config_store: ArcConfiguration,
    mut input_commands_rx: Receiver<SystemCommand>,
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
                    info!("Shutting down system");
                    std::process::Command::new("/usr/sbin/poweroff")
                        .spawn()
                        .expect("halt command failed");
                }
                RestartSystem => {
                    info!("Restarting system");
                    std::process::Command::new("/usr/sbin/reboot")
                        .spawn()
                        .expect("halt command failed");
                }
                RestartRSPlayer => {
                    info!("Restarting RSPlayer");
                    exit(1);
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
