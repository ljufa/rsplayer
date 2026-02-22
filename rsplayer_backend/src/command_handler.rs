use std::process::exit;
use std::sync::Arc;

use log::debug;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::Receiver;

use api_models::common::MetadataCommand::{QueryLocalFiles, RescanMetadata};
use api_models::common::PlayerCommand::{
    CyclePlaybackMode, Next, Pause, Play, PlayItem, Prev, QueryCurrentPlayerInfo, Seek, SeekBackward, SeekForward,
    Stop, TogglePlay,
};
use api_models::common::PlaylistCommand::{QueryAlbumItems, QueryPlaylist, QueryPlaylistItems, SaveQueueAsPlaylist};
use api_models::common::QueueCommand::{
    self, AddLocalLibDirectory, AddSongToQueue, ClearQueue, LoadAlbumInQueue, LoadArtistInQueue, LoadPlaylistInQueue,
    LoadSongToQueue, QueryCurrentQueue, QueryCurrentSong, RemoveItem,
};
use api_models::common::SystemCommand::{PowerOff, RestartRSPlayer, RestartSystem, SetVol, VolDown, VolUp};
use api_models::common::UserCommand::{Metadata, Player, Playlist, Queue, UpdateDsp};
use api_models::common::{MetadataCommand, MetadataLibraryItem, SystemCommand, UserCommand};
use api_models::playlist::PlaylistType;
use api_models::state::StateChangeEvent;
use rsplayer_config::ArcConfiguration;
use rsplayer_hardware::audio_device::audio_service::ArcAudioInterfaceSvc;
use rsplayer_hardware::usb::ArcUsbService;
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
    config_store: ArcConfiguration,
    mut input_commands_rx: Receiver<UserCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    loop {
        let Some(cmd) = input_commands_rx.recv().await else {
            debug!("Wait in loop");
            continue;
        };
        debug!("Received command {cmd:?}");
        let sender = &state_changes_sender.clone();
        match cmd {
            /*
             * Player commands
             */
            Player(Play) => {
                player_service.stop_current_song();
                player_service.play_from_current_queue_song();
                debug!("Play from current song command processed");
            }
            Player(PlayItem(id)) => {
                player_service.play_song(&id);
            }
            Player(Pause | Stop) => {
                player_service.stop_current_song();
            }
            Player(TogglePlay) => {
                player_service.toggle_play_pause();
            }

            Player(Next) => {
                player_service.play_next_song();
            }
            Player(Prev) => {
                player_service.play_prev_song();
            }
            Player(Seek(sec)) => {
                player_service.seek_current_song(sec);
            }
            Player(SeekForward) => {
                player_service.seek_relative(10);
            }
            Player(SeekBackward) => {
                player_service.seek_relative(-10);
            }
            Player(CyclePlaybackMode) => {
                sender
                    .send(StateChangeEvent::PlaybackModeChangedEvent(
                        queue_service.cycle_playback_mode(),
                    ))
                    .unwrap();
            }
            Player(QueryCurrentPlayerInfo) => {
                let mode = queue_service.get_playback_mode();
                state_changes_sender
                    .send(StateChangeEvent::PlaybackModeChangedEvent(mode))
                    .unwrap();
                let settings = config_store.get_settings();
                state_changes_sender
                    .send(StateChangeEvent::VuMeterEnabledEvent(
                        settings.rs_player_settings.vu_meter_enabled,
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
                let songs = if playlist_id == "most_played" {
                    let all = metadata_service.get_most_played_songs(100);
                    all.into_iter().skip(page_no * 20).take(20).collect()
                } else if playlist_id == "liked" {
                    let all = metadata_service.get_liked_songs(100);
                    all.into_iter().skip(page_no * 20).take(20).collect()
                } else {
                    playlist_service
                        .get_playlist_page_by_name(&playlist_id, page_no * 20, 20)
                        .items
                };
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
                if let Some(first_most_played) = metadata_service.get_most_played_songs(1).first() {
                    let pl = api_models::playlist::Playlist {
                        id: "most_played".to_string(),
                        name: "Most Played".to_string(),
                        description: Some("Your most played tracks".to_string()),
                        image: first_most_played.image_id.clone().map(|id| format!("/artwork/{id}")),
                        owner_name: None,
                    };
                    pls.items.push(PlaylistType::MostPlayed(pl));
                }

                if let Some(first_liked) = metadata_service.get_liked_songs(1).first() {
                    let pl = api_models::playlist::Playlist {
                        id: "liked".to_string(),
                        name: "Liked".to_string(),
                        description: Some("Songs you liked".to_string()),
                        image: first_liked.image_id.clone().map(|id| format!("/artwork/{id}")),
                        owner_name: None,
                    };
                    pls.items.push(PlaylistType::Liked(pl));
                }

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
                }
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
                }
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
            UpdateDsp(dsp_settings) => {
                player_service.update_dsp_settings(&dsp_settings);
                let mut settings = config_store.get_settings();
                settings.rs_player_settings.dsp_settings = dsp_settings;
                config_store.save_settings(&settings);
                state_changes_sender
                    .send(StateChangeEvent::NotificationSuccess(
                        "DSP settings updated and saved".to_string(),
                    ))
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
    usb_service: Option<ArcUsbService>,
    mut input_commands_rx: Receiver<SystemCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    loop {
        if let Some(cmd) = input_commands_rx.recv().await {
            debug!("Received command {cmd:?}");
            match cmd {
                SystemCommand::SetFirmwarePower(val) => {
                    if let Some(service) = &usb_service {
                        if let Err(e) = service.send_power_command(val) {
                            error!("Failed to send power command: {e}");
                        }
                    }
                }
                SetVol(val) => {
                    let nv = ai_service.set_volume(val);
                    state_changes_sender
                        .send(StateChangeEvent::VolumeChangeEvent(nv))
                        .expect("Send event failed.");
                }
                VolUp => {
                    let nv = ai_service.volume_up();
                    if nv.current > 0 {
                        state_changes_sender
                            .send(StateChangeEvent::VolumeChangeEvent(nv))
                            .expect("Send event failed.");
                    }
                }
                VolDown => {
                    let nv = ai_service.volume_down();
                    if nv.current > 0 {
                        state_changes_sender
                            .send(StateChangeEvent::VolumeChangeEvent(nv))
                            .expect("Send event failed.");
                    }
                }
                PowerOff => {
                    info!("Shutting down system");
                    _ = std::process::Command::new("/usr/sbin/poweroff")
                        .spawn()
                        .expect("halt command failed")
                        .wait();
                }
                RestartSystem => {
                    info!("Restarting system");
                    _ = std::process::Command::new("/usr/sbin/reboot")
                        .spawn()
                        .expect("halt command failed")
                        .wait();
                }
                RestartRSPlayer => {
                    info!("Restarting RSPlayer");
                    exit(1);
                }
                SystemCommand::QueryCurrentVolume => {
                    let vol = ai_service.get_volume();
                    state_changes_sender
                        .send(StateChangeEvent::VolumeChangeEvent(vol))
                        .unwrap();
                }
            }
        }
    }
}
