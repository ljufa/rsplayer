use std::borrow::BorrowMut;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::time::Duration;

use api_models::player::Song;
use api_models::playlist::{
    Category, DynamicPlaylistsPage, Playlist, PlaylistPage, PlaylistType, Playlists,
};
use api_models::settings::MpdSettings;
use api_models::state::{
    PlayerInfo, PlayerState, PlayingContext, PlayingContextQuery, SongProgress,
};
use mpd::{Client, Query, Song as MpdSong};
use num_traits::ToPrimitive;

use crate::common::Result;

use super::Player;

const SAVED_PL_PREFIX: &str = "mpd_playlist_saved_";
const BY_GENRE_PL_PREFIX: &str = "mpd_playlist_by_genre_";
const BY_DATE_PL_PREFIX: &str = "mpd_playlist_by_date_";
const BY_ARTIST_PL_PREFIX: &str = "mpd_playlist_by_artist_";
const BY_FOLDER_PL_PREFIX: &str = "mpd_playlist_by_folder_";

const CATEGORY_ID_BY_GENRE: &str = "mpd_category_by_genre";
const CATEGORY_ID_BY_DATE: &str = "mpd_category_by_date";
const CATEGORY_ID_BY_ARTIST: &str = "mpd_category_by_artist";
const CATEGORY_ID_BY_FOLDER: &str = "mpd_category_by_folder";
const PAGE_SIZE: usize = 80;
const MPD_CONF_FILE_TEMPLATE: &str = r#"
playlist_directory        "/var/lib/mpd/playlists"
db_file                   "/var/lib/mpd/tag_cache"
state_file                "/var/lib/mpd/state"
sticker_file              "/var/lib/mpd/sticker.sql"
music_directory           "{music_directory}"

bind_to_address           "0.0.0.0"
port                      "6600"
log_level                 "default"
restore_paused            "yes"
auto_update               "yes"
follow_outside_symlinks   "yes"
follow_inside_symlinks    "yes"
zeroconf_enabled          "no"
filesystem_charset        "UTF-8"

input {
  plugin "curl"
}

audio_output {
  type                    "alsa"
  name                    "audio device"
  device                  "{audio_device}"
  mixer_type              "none"
  replay_gain_handler     "none"
}
"#;

#[derive(Debug)]
pub struct MpdPlayerClient {
    mpd_client: Client,
    progress: SongProgress,
    all_songs: Vec<Song>,
    mpd_settings: MpdSettings,
}

impl MpdPlayerClient {
    pub fn new(mpd_settings: &MpdSettings) -> Result<MpdPlayerClient> {
        if !mpd_settings.enabled {
            return Err(failure::err_msg("MPD player integration is disabled."));
        }

        Ok(MpdPlayerClient {
            mpd_client: create_client(mpd_settings)?,
            progress: SongProgress::default(),
            all_songs: vec![],
            mpd_settings: mpd_settings.clone(),
        })
    }

    pub fn ensure_mpd_server_configuration(
        &mut self,
        audio_device_name: &str,
        music_directory: &str,
    ) -> Result<()> {
        let existing_content = std::fs::read_to_string("/etc/mpd.conf")?;
        if self.mpd_settings.override_external_configuration {
            let mut new_content =
                MPD_CONF_FILE_TEMPLATE.replace("{music_directory}", music_directory);
            new_content = new_content.replace("{audio_device}", audio_device_name);
            if new_content != existing_content {
                std::fs::copy("/etc/mpd.conf", "/tmp/mpd.conf.rsplayer.origin")?;
                std::fs::write("/etc/mpd.conf", new_content)?;
                std::process::Command::new("systemctl")
                    .arg("restart")
                    .arg("mpd")
                    .spawn()?;
                self.mpd_client = create_client(&self.mpd_settings)?;
            }
        }
        Ok(())
    }

    fn try_with_reconnect_result<F, R>(&mut self, mut command: F) -> Result<R>
    where
        F: FnMut(&mut Client) -> mpd::error::Result<R>,
    {
        let mut result = command(self.mpd_client.borrow_mut());
        if result.is_err() {
            match Client::connect(self.mpd_settings.get_server_url().as_str()) {
                Ok(cl) => {
                    self.mpd_client = cl;
                    result = command(self.mpd_client.borrow_mut());
                }
                Err(e) => result = Err(e),
            }
        }
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(failure::format_err!("{}", e)),
        }
    }

    fn execute_mpd_command<F, R>(
        &mut self,
        command: &str,
        mut transform_response_fn: F,
    ) -> Option<R>
    where
        F: FnMut(&mut BufReader<&mut TcpStream>) -> Option<R>,
    {
        let mut full_cmd = String::new();
        full_cmd.push_str(command);
        full_cmd.push('\n');
        let mut client = create_socket_client(&self.mpd_settings.get_server_url());
        client
            .write_all(full_cmd.as_bytes())
            .expect("Can't write to socket");

        let mut reader = BufReader::new(&mut client);
        transform_response_fn(&mut reader)
    }

    fn get_songs_in_queue(&mut self) -> Vec<Song> {
        self.execute_mpd_command("playlistinfo", |reader| Some(mpd_response_to_songs(reader)))
            .unwrap()
    }

    fn get_all_songs_in_library(&mut self) -> Vec<Song> {
        self.execute_mpd_command("listallinfo", |reader| Some(mpd_response_to_songs(reader)))
            .unwrap()
    }

    fn get_songs_in_playlist(&mut self, playlist_name: String) -> Vec<Song> {
        self.execute_mpd_command(
            format!("listplaylistinfo \"{playlist_name}\"").as_str(),
            |reader| Some(mpd_response_to_songs(reader)),
        )
        .unwrap()
    }
}

impl Player for MpdPlayerClient {
    fn play_current_track(&mut self) {
        _ = self.try_with_reconnect_result(mpd::Client::play);
    }

    fn pause_current_track(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.pause(true));
    }

    fn play_next_track(&mut self) {
        _ = self.try_with_reconnect_result(mpd::Client::next);
    }

    fn play_prev_track(&mut self) {
        _ = self.try_with_reconnect_result(mpd::Client::prev);
    }

    fn stop_current_track(&mut self) {
        _ = self.try_with_reconnect_result(mpd::Client::stop);
    }

    fn shutdown(&mut self) {
        info!("Shutting down MPD player!");
        self.stop_current_track();
        _ = self.mpd_client.close();
        info!("MPD player shutdown finished!");
    }

    fn seek_current_track(&mut self, seconds: i8) {
        let result = self.try_with_reconnect_result(mpd::Client::status);
        if let Ok(status) = result {
            //todo: implement protection against going of the range
            let position = status.elapsed.unwrap().num_seconds() + i64::from(seconds);
            _ = self.mpd_client.rewind(position);
        };
    }

    fn toggle_random_play(&mut self) {
        let status = self.try_with_reconnect_result(mpd::Client::status);
        if let Ok(status) = status {
            _ = self.mpd_client.random(!status.random);
        }
    }

    fn load_playlist_in_queue(&mut self, pl_id: String) {
        if pl_id.starts_with(SAVED_PL_PREFIX) {
            let pl_id = pl_id.replace(SAVED_PL_PREFIX, "");
            _ = self.try_with_reconnect_result(|client| {
                _ = client.clear();
                client.load(pl_id.clone(), ..)
            });
        } else if pl_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_GENRE_PL_PREFIX, "");
            _ = self.mpd_client.clear();
            _ = self
                .mpd_client
                .findadd(Query::new().and(mpd::Term::Tag("Genre".into()), pl_id));
        } else if pl_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_DATE_PL_PREFIX, "");
            _ = self.mpd_client.clear();
            _ = self
                .mpd_client
                .findadd(Query::new().and(mpd::Term::Tag("Date".into()), pl_id));
        } else if pl_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_ARTIST_PL_PREFIX, "");
            _ = self.mpd_client.clear();
            _ = self
                .mpd_client
                .findadd(Query::new().and(mpd::Term::Tag("Artist".into()), pl_id));
        } else if pl_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_FOLDER_PL_PREFIX, "");
            _ = self.mpd_client.clear();
            self.all_songs
                .iter()
                .filter(|s| {
                    s.file
                        .split('/')
                        .nth(1)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(pl_id.as_str())
                })
                .for_each(|s| {
                    _ = self
                        .mpd_client
                        .findadd(Query::new().and(mpd::Term::File, s.file.clone()));
                });
        }
        _ = self.mpd_client.play();
    }

    fn load_album_in_queue(&mut self, _album_id: String) {
        todo!()
    }

    fn play_track(&mut self, id: String) {
        if let Ok(id) = id.parse::<u32>() {
            _ = self.try_with_reconnect_result(|client| client.switch(mpd::song::Id(id)));
        }
    }

    fn remove_track_from_queue(&mut self, id: String) {
        if let Ok(id) = id.parse::<u32>() {
            _ = self.try_with_reconnect_result(|client| client.delete(mpd::song::Id(id)));
        }
    }

    fn get_song_progress(&mut self) -> SongProgress {
        self.progress.clone()
    }

    fn get_current_track(&mut self) -> Option<Song> {
        let result = self.try_with_reconnect_result(mpd::Client::currentsong);
        let song = result.unwrap_or(None);
        song.map(|s| map_song(&s))
    }

    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        let status = self.try_with_reconnect_result(mpd::Client::status);
        trace!("Mpd Status is {:?}", status);
        if let Ok(status) = status {
            let time = status.time.map_or((Duration::ZERO, Duration::ZERO), |t| {
                (
                    Duration::from_nanos(
                        t.0.num_nanoseconds()
                            .unwrap_or_default()
                            .to_u64()
                            .unwrap_or_default(),
                    ),
                    Duration::from_nanos(
                        t.1.num_nanoseconds()
                            .unwrap_or_default()
                            .to_u64()
                            .unwrap_or_default(),
                    ),
                )
            });
            self.progress = SongProgress {
                total_time: time.1,
                current_time: time.0,
            };
            Some(PlayerInfo {
                audio_format_bit: status.audio.map(|f| f.bits),
                audio_format_rate: status.audio.map(|f| f.rate),
                audio_format_channels: status.audio.map(|f| u32::from(f.chans)),
                random: Some(status.random),
                state: Some(map_state(status.state)),
            })
        } else {
            error!("Error while getting mpd status {:?}", status);
            None
        }
    }

    #[tracing::instrument]
    fn get_playing_context(&mut self, query: PlayingContextQuery) -> Option<PlayingContext> {
        let mut pc = PlayingContext {
            id: "1".to_string(),
            name: "Queue".to_string(),
            player_type: api_models::common::PlayerType::MPD,
            context_type: api_models::state::PlayingContextType::Playlist {
                description: None,
                public: None,
                snapshot_id: "1".to_string(),
            },
            playlist_page: None,
            image_url: None,
        };
        match query {
            PlayingContextQuery::WithSearchTerm(term, offset) => {
                let mut songs = self.get_songs_in_queue();
                if term.len() > 2 {
                    songs.retain(|s| s.all_text().to_lowercase().contains(&term.to_lowercase()));
                }
                let page = PlaylistPage {
                    total: songs.len(),
                    offset: offset + PAGE_SIZE,
                    limit: PAGE_SIZE,
                    items: songs
                        .into_iter()
                        .skip(offset.to_usize().unwrap_or_default())
                        .take(PAGE_SIZE.to_usize().unwrap_or_default())
                        .collect(),
                };
                pc.playlist_page = Some(page);
            }
            PlayingContextQuery::CurrentSongPage => {
                let mut songs = self.get_songs_in_queue();
                if let Some(cs) = &self.get_current_track() {
                    songs = songs
                        .into_iter()
                        .skip_while(|s| s.id != cs.id)
                        .take(PAGE_SIZE)
                        .collect();
                }
                let page = PlaylistPage {
                    total: songs.len(),
                    offset: 0,
                    limit: PAGE_SIZE,
                    items: songs,
                };
                pc.playlist_page = Some(page);
            }
            PlayingContextQuery::IgnoreSongs => {}
        }
        Some(pc)
    }

    fn get_playlist_categories(&mut self) -> Vec<Category> {
        vec![
            Category {
                id: CATEGORY_ID_BY_ARTIST.to_string(),
                name: "By Artist".to_string(),
                icon: String::new(),
            },
            Category {
                id: CATEGORY_ID_BY_DATE.to_string(),
                name: "By Date".to_string(),
                icon: String::new(),
            },
            Category {
                id: CATEGORY_ID_BY_GENRE.to_string(),
                name: "By Genre".to_string(),
                icon: String::new(),
            },
            Category {
                id: CATEGORY_ID_BY_FOLDER.to_string(),
                name: "By Directory".to_string(),
                icon: String::new(),
            },
        ]
    }

    fn get_static_playlists(&mut self) -> Playlists {
        // saved pls
        let pls = self
            .try_with_reconnect_result(mpd::Client::playlists)
            .unwrap_or_default();
        let items: Vec<PlaylistType> = pls
            .iter()
            .map(|p| {
                PlaylistType::Saved(Playlist {
                    name: p.name.clone(),
                    id: format!("{}{}", SAVED_PL_PREFIX, p.name),
                    description: None,
                    image: None,
                    owner_name: None,
                })
            })
            .collect();
        Playlists { items }
    }

    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage> {
        if self.all_songs.is_empty() {
            self.all_songs = self.get_all_songs_in_library();
        };
        let mut result = vec![];
        for category_id in category_ids {
            result.push(match category_id.as_str() {
                CATEGORY_ID_BY_ARTIST => DynamicPlaylistsPage {
                    category_id: CATEGORY_ID_BY_ARTIST.to_string(),
                    playlists: get_playlists_by_artist(&self.all_songs, offset, limit),
                    offset,
                    limit,
                },
                CATEGORY_ID_BY_DATE => DynamicPlaylistsPage {
                    category_id: CATEGORY_ID_BY_DATE.to_string(),
                    playlists: get_playlists_by_date(&self.all_songs, offset, limit),
                    offset,
                    limit,
                },
                CATEGORY_ID_BY_GENRE => DynamicPlaylistsPage {
                    category_id: CATEGORY_ID_BY_GENRE.to_string(),
                    playlists: get_playlists_by_genre(&self.all_songs, offset, limit),
                    offset,
                    limit,
                },
                CATEGORY_ID_BY_FOLDER => DynamicPlaylistsPage {
                    category_id: CATEGORY_ID_BY_FOLDER.to_string(),
                    playlists: get_playlists_by_folder(&self.all_songs, offset, limit),
                    offset,
                    limit,
                },
                &_ => {
                    todo!()
                }
            });
        }
        result
    }

    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<Song> {
        if playlist_id.starts_with(SAVED_PL_PREFIX) {
            let pl_name = playlist_id.replace(SAVED_PL_PREFIX, "");
            self.get_songs_in_playlist(pl_name)
                .into_iter()
                .take(100)
                .collect()
        } else if playlist_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_GENRE_PL_PREFIX, "");
            self.all_songs
                .iter()
                .filter(|s| s.genre.as_ref().map_or(false, |g| *g == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.all_songs
                .iter()
                .filter(|s| s.artist.as_ref().map_or(false, |a| *a == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_DATE_PL_PREFIX, "");
            self.all_songs
                .iter()
                .filter(|s| s.date.as_ref().map_or(false, |d| *d == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.all_songs
                .iter()
                .filter(|s| s.file.split('/').nth(1).map_or(false, |d| *d == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }

    fn load_track_in_queue(&mut self, song_id: String) {
        self.clear_queue();
        self.add_track_in_queue(song_id);
        self.play_current_track();
    }

    fn add_track_in_queue(&mut self, song_id: String) {
        self.execute_mpd_command(
            format!("add \"{song_id}\"").as_str(),
            |reader| -> Option<String> {
                let mut out: String = String::new();
                reader.read_line(&mut out).expect("Failed to read response");
                debug!("Response line {}", out);
                None
            },
        );
    }

    fn clear_queue(&mut self) {
        _ = self.mpd_client.clear();
    }

    fn save_queue_as_playlist(&mut self, playlist_name: String) {
        _ = self.mpd_client.save(playlist_name);
    }

    fn rescan_metadata(&mut self) {
        _ = self.mpd_client.rescan();
    }
}

fn get_playlists_by_genre(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    let mut items = vec![];
    let mut genres: Vec<String> = all_songs
        .iter()
        .filter_map(|s| s.genre.clone())
        .filter(|g| g.starts_with(char::is_alphabetic))
        .collect();
    genres.sort();
    genres.dedup();
    genres
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|g| {
            items.push(Playlist {
                name: g.clone(),
                id: format!("{}{}", BY_GENRE_PL_PREFIX, g),
                description: Some("Songs by genre ".to_string() + g),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_date(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    // dynamic pls
    let mut items = vec![];
    let mut dates: Vec<String> = all_songs.iter().filter_map(|s| s.date.clone()).collect();
    dates.sort();
    dates.dedup();
    dates.reverse();
    dates
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|date| {
            items.push(Playlist {
                name: date.clone(),
                id: format!("{}{}", BY_DATE_PL_PREFIX, date),
                description: Some("Songs by date ".to_string() + date),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_artist(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    let mut items = vec![];
    let mut artists: Vec<String> = all_songs
        .iter()
        .filter_map(|s| s.artist.clone())
        .filter(|art| art.starts_with(char::is_alphabetic))
        .collect();
    artists.sort();
    artists.dedup();
    artists
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|art| {
            items.push(Playlist {
                name: art.clone(),
                id: format!("{}{}", BY_ARTIST_PL_PREFIX, art),
                description: Some("Songs by artist ".to_string() + art),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn get_playlists_by_folder(all_songs: &[Song], offset: u32, limit: u32) -> Vec<Playlist> {
    let mut items = vec![];
    let second_level_folders: HashSet<String> = all_songs
        .iter()
        .map(|s| s.file.clone())
        .map(|file| file.split('/').nth(1).unwrap_or_default().to_string())
        .collect();
    second_level_folders
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .for_each(|folder| {
            items.push(Playlist {
                name: folder.clone(),
                id: format!("{}{}", BY_FOLDER_PL_PREFIX, folder),
                description: Some("Songs by dir ".to_string() + folder),
                image: None,
                owner_name: None,
            });
        });
    items
}

fn map_song(song: &MpdSong) -> Song {
    trace!("Song is {:?}", song);
    Song {
        file: song.file.clone(),
        title: song.title.clone(),
        id: song.place.map_or(String::new(), |p| p.id.0.to_string()),
        album: tag_to_value(song, "Album"),
        artist: tag_to_value(song, "Artist"),
        genre: tag_to_value(song, "Genre"),
        date: tag_to_value(song, "Date"),
        album_artist: tag_to_value(song, "AlbumArtist"),
        composer: tag_to_value(song, "Composer"),
        disc: tag_to_value(song, "Disc"),
        label: tag_to_value(song, "Label"),
        last_modified: tag_to_value(song, "Last-Modified"),
        performer: tag_to_value(song, "Performer"),
        time: tag_to_value(song, "Time").map(|t| Duration::from_secs(t.parse::<u64>().unwrap())),
        track: tag_to_value(song, "Track"),
        tags: HashMap::new(),
        image_url: None,
    }
}

fn tag_to_value(song: &MpdSong, key: &str) -> Option<String> {
    song.tags.iter().find(|t| t.0 == key).map(|kv| kv.1.clone())
}

const fn map_state(mpd_state: mpd::status::State) -> PlayerState {
    match mpd_state {
        mpd::State::Stop => PlayerState::STOPPED,
        mpd::State::Play => PlayerState::PLAYING,
        mpd::State::Pause => PlayerState::PAUSED,
    }
}

impl Drop for MpdPlayerClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn create_client(mpd_settings: &MpdSettings) -> Result<Client> {
    let mut tries = 0;
    let mut connection = None;
    let mut last_error: Option<mpd::error::Error> = None;
    while tries < 5 {
        tries += 1;
        info!(
            "Trying to connect to MPD server {}. Attempt no: {}",
            mpd_settings.get_server_url(),
            tries,
        );
        let conn = Client::connect(mpd_settings.get_server_url().as_str());
        match conn {
            Ok(conn) => {
                info!("Mpd client created");
                connection = Some(conn);
                break;
            }
            Err(e) => {
                error!("Failed to connect to mpd server {}", e);
                last_error = Some(e);
            }
        }
    }
    match connection {
        Some(c) => Ok(c),
        None => Err(failure::format_err!(
            "Failed connect to to MPD server! [{}]",
            last_error.unwrap()
        )),
    }
}

fn create_socket_client(mpd_server_url: &str) -> TcpStream {
    let client = TcpStream::connect_timeout(
        &SocketAddr::from_str(mpd_server_url).unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();

    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("Failed to set read timeout");

    client
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("Failed to set write timeout");
    client
}
fn mpd_response_to_songs(reader: &mut BufReader<&mut TcpStream>) -> Vec<Song> {
    let mut read_buffer = String::new();
    // skip header lines
    for _ in 1..15 {
        read_buffer.clear();
        let res = reader.read_line(&mut read_buffer).unwrap_or_default();
        if res < 5 || read_buffer.starts_with("file") {
            break;
        }
    }
    let mut result = Vec::<Song>::new();
    loop {
        if read_buffer.starts_with("file:") {
            let mut song = Song {
                file: to_opt_string(read_buffer.split_once(':').unwrap_or_default().1)
                    .unwrap_or_default(),
                ..Default::default()
            };
            'song: loop {
                let mut song_buffer = String::new();
                reader.read_line(&mut song_buffer).unwrap_or_default();

                // end of response
                if song_buffer == "OK\n" {
                    read_buffer.clear();
                    break 'song;
                }
                if !song_buffer.contains(':') {
                    continue;
                }

                let pair = song_buffer.split_once(':').unwrap_or_default();
                let key = pair.0;
                let value = pair.1;
                match key {
                    "Artist" => song.artist = to_opt_string(value),
                    "Title" => song.title = to_opt_string(value),
                    "Genre" => song.genre = to_opt_string(value),
                    "Album" => song.album = to_opt_string(value),
                    "Date" => song.date = to_opt_string(value),
                    "Track" => song.track = to_opt_string(value),
                    "Time" => {
                        song.time = to_opt_string(value)
                            .map(|f| Duration::from_secs(f.parse::<u64>().unwrap()));
                    }
                    "Id" => song.id = value.trim().to_string(),
                    "Last-Modified" => song.last_modified = to_opt_string(value),
                    "Performer" => song.performer = to_opt_string(value),
                    "Composer" => song.composer = to_opt_string(value),
                    "AlbumArtist" => song.album_artist = to_opt_string(value),
                    "Disc" => song.disc = to_opt_string(value),
                    "Label" => song.label = to_opt_string(value),
                    "Range" | "Pos" | "duration" => {}
                    "file" => {
                        read_buffer = song_buffer;
                        break 'song;
                    }
                    &_ => {
                        trace!("Unmatched:|{}|", song_buffer);
                        song.tags.insert(
                            String::from_str(key).unwrap(),
                            to_opt_string(value).unwrap(),
                        );
                    }
                }
            }
            result.push(song);
        } else {
            break;
        }
    }
    result
}

fn to_opt_string(value: &str) -> Option<String> {
    String::from_str(value.replace('\"', "").trim()).ok()
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        io::{self, BufRead, Write},
    };

    #[test]
    fn parse_config() {
        let in_file = fs::File::open("/home/dlj/myworkspace/rsplayer/.run/mpd.conf").unwrap();
        let out_file =
            fs::File::create("/home/dlj/myworkspace/rsplayer/.run/mpd_new.conf").unwrap();
        let mut out_buffer = io::LineWriter::new(out_file);

        let lines = io::BufReader::new(in_file).lines();
        for line in lines {
            let line = line.unwrap();
            //let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut out_line = line.clone();
            if line.contains("music_directory") {
                out_line = "music_directory\t\t\"/home/dragan/music\"".to_owned();
            }
            if line.trim().starts_with("audio_output") {}
            _ = out_buffer.write_fmt(format_args!("{}\n", out_line));
        }
        _ = out_buffer.flush();
    }
}
