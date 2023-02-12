use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api_models::num_traits::ToPrimitive;
use api_models::player::Song;
use api_models::playlist::{
    Category, DynamicPlaylistsPage, Playlist, PlaylistPage, PlaylistType, Playlists,
};
use api_models::settings::MpdSettings;
use api_models::state::{
    PlayerInfo, PlayerState, PlayingContext, PlayingContextQuery, SongProgress,
};
use log::{debug, error, info};
use mpd::{Client, Query, Song as MpdSong};

use anyhow::Result;

use crate::{
    get_dynamic_playlists, get_playlist_categories, BY_ARTIST_PL_PREFIX, BY_DATE_PL_PREFIX,
    BY_FOLDER_PL_PREFIX, BY_GENRE_PL_PREFIX, SAVED_PL_PREFIX,
};

use super::Player;

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

const BY_FOLDER_DEPTH: usize = 1;
#[derive(Debug)]
pub struct MpdPlayerClient {
    mpd_client: Arc<Mutex<Client>>,
    progress: Arc<Mutex<SongProgress>>,
    mpd_settings: MpdSettings,
}

impl MpdPlayerClient {
    pub fn new(mpd_settings: &MpdSettings) -> Result<MpdPlayerClient> {
        if !mpd_settings.enabled {
            return Err(anyhow::format_err!("MPD player integration is disabled."));
        }
        Ok(MpdPlayerClient {
            mpd_client: Arc::new(Mutex::new(create_client(mpd_settings)?)),
            progress: Arc::new(Mutex::new(SongProgress::default())),
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
                *self.mpd_client.lock().unwrap() = create_client(&self.mpd_settings)?;
            }
        }
        Ok(())
    }

    fn execute_mpd_command<F, R>(&self, command: &str, mut transform_response_fn: F) -> Option<R>
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

    fn get_songs_in_queue(&self) -> Vec<Song> {
        self.execute_mpd_command("playlistinfo", |reader| Some(mpd_response_to_songs(reader)))
            .unwrap()
    }

    fn get_all_songs_in_library(&self) -> Vec<Song> {
        self.execute_mpd_command("listallinfo", |reader| Some(mpd_response_to_songs(reader)))
            .unwrap()
    }

    fn get_songs_in_playlist(&self, playlist_name: &str) -> Vec<Song> {
        self.execute_mpd_command(
            format!("listplaylistinfo \"{playlist_name}\"").as_str(),
            |reader| Some(mpd_response_to_songs(reader)),
        )
        .unwrap()
    }
}

impl Player for MpdPlayerClient {
    fn play_from_current_queue_song(&self) {
        _ = self.mpd_client.lock().unwrap().play();
    }

    fn pause_current_song(&self) {
        _ = self.mpd_client.lock().unwrap().pause(true);
    }

    fn play_next_song(&self) {
        _ = self.mpd_client.lock().unwrap().next();
    }

    fn play_prev_song(&self) {
        _ = self.mpd_client.lock().unwrap().prev();
    }

    fn stop_current_song(&self) {
        _ = self.mpd_client.lock().unwrap().stop();
    }

    fn seek_current_song(&self, seconds: i8) {
        let result = self.mpd_client.lock().unwrap().status();
        if let Ok(status) = result {
            //todo: implement protection against going of the range
            let position = status.elapsed.unwrap().num_seconds() + i64::from(seconds);
            _ = self.mpd_client.lock().unwrap().rewind(position);
        };
    }

    fn toggle_random_play(&self) {
        let status = self.mpd_client.lock().unwrap().status();
        if let Ok(status) = status {
            _ = self.mpd_client.lock().unwrap().random(!status.random);
        }
    }

    fn load_playlist_in_queue(&self, pl_id: &str) {
        if pl_id.starts_with(SAVED_PL_PREFIX) {
            let pl_id = pl_id.replace(SAVED_PL_PREFIX, "");
            _ = self.mpd_client.lock().unwrap().clear();
            _ = self.mpd_client.lock().unwrap().load(pl_id, ..);
        } else if pl_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_GENRE_PL_PREFIX, "");
            _ = self.mpd_client.lock().unwrap().clear();
            _ = self
                .mpd_client
                .lock()
                .unwrap()
                .findadd(Query::new().and(mpd::Term::Tag("Genre".into()), pl_id));
        } else if pl_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_DATE_PL_PREFIX, "");
            _ = self.mpd_client.lock().unwrap().clear();
            _ = self
                .mpd_client
                .lock()
                .unwrap()
                .findadd(Query::new().and(mpd::Term::Tag("Date".into()), pl_id));
        } else if pl_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_ARTIST_PL_PREFIX, "");
            _ = self.mpd_client.lock().unwrap().clear();
            _ = self
                .mpd_client
                .lock()
                .unwrap()
                .findadd(Query::new().and(mpd::Term::Tag("Artist".into()), pl_id));
        } else if pl_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_id = pl_id.replace(BY_FOLDER_PL_PREFIX, "");
            _ = self.mpd_client.lock().unwrap().clear();
            self.get_all_songs_in_library()
                .iter()
                .filter(|s| {
                    s.file
                        .split('/')
                        .nth(BY_FOLDER_DEPTH)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(pl_id.as_str())
                })
                .for_each(|s| {
                    _ = self
                        .mpd_client
                        .lock()
                        .unwrap()
                        .findadd(Query::new().and(mpd::Term::File, s.file.clone()));
                });
        }
        _ = self.mpd_client.lock().unwrap().play();
    }

    fn load_album_in_queue(&self, _album_id: &str) {
        // todo!()
    }

    fn play_song(&self, id: &str) {
        if let Ok(id) = id.parse::<u32>() {
            _ = self.mpd_client.lock().unwrap().switch(mpd::song::Id(id));
        }
    }

    fn remove_song_from_queue(&self, id: &str) {
        if let Ok(id) = id.parse::<u32>() {
            _ = self.mpd_client.lock().unwrap().delete(mpd::song::Id(id));
        }
    }

    fn get_song_progress(&self) -> SongProgress {
        self.progress.lock().unwrap().clone()
    }

    fn get_current_song(&self) -> Option<Song> {
        let result = self.mpd_client.lock().unwrap().currentsong();
        let song = result.unwrap_or(None);
        song.map(|s| map_song(&s))
    }

    fn get_player_info(&self) -> Option<PlayerInfo> {
        let status = self.mpd_client.lock().unwrap().status();
        debug!("Mpd Status is {:?}", status);
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
            *self.progress.lock().unwrap() = SongProgress {
                total_time: time.1,
                current_time: time.0,
            };
            Some(PlayerInfo {
                audio_format_bit: status.audio.map(|f| f.bits.to_u32().unwrap_or_default()),
                audio_format_rate: status.audio.map(|f| f.rate),
                audio_format_channels: status.audio.map(|f| u32::from(f.chans)),
                random: Some(status.random),
                state: Some(map_state(status.state)),
                codec: None
            })
        } else {
            error!("Error while getting mpd status {:?}", status);
            *self.mpd_client.lock().unwrap() = create_client(&self.mpd_settings).unwrap();
            None
        }
    }

    fn get_playing_context(&self, query: PlayingContextQuery) -> Option<PlayingContext> {
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
                if let Some(cs) = &self.get_current_song() {
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

    fn get_playlist_categories(&self) -> Vec<Category> {
        get_playlist_categories()
    }

    fn get_static_playlists(&self) -> Playlists {
        // saved pls
        let pls = self
            .mpd_client
            .lock()
            .unwrap()
            .playlists()
            .unwrap_or_default();
        let items: Vec<PlaylistType> = pls
            .iter()
            .map(|p| {
                PlaylistType::Saved(Playlist {
                    name: p.name.clone(),
                    id: format!("{SAVED_PL_PREFIX}{}", p.name),
                    description: None,
                    image: None,
                    owner_name: None,
                })
            })
            .collect();
        Playlists { items }
    }

    fn get_dynamic_playlists(
        &self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage> {
        get_dynamic_playlists(
            category_ids,
            &self.get_all_songs_in_library(),
            offset,
            limit,
            2,
        )
    }

    fn get_playlist_items(&self, playlist_id: &str, _page_no: usize) -> Vec<Song> {
        if playlist_id.starts_with(SAVED_PL_PREFIX) {
            let pl_name = playlist_id.replace(SAVED_PL_PREFIX, "");
            self.get_songs_in_playlist(&pl_name)
                .into_iter()
                .take(100)
                .collect()
        } else if playlist_id.starts_with(BY_GENRE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_GENRE_PL_PREFIX, "");
            self.get_all_songs_in_library()
                .iter()
                .filter(|s| s.genre.as_ref().map_or(false, |g| *g == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_ARTIST_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_ARTIST_PL_PREFIX, "");
            self.get_all_songs_in_library()
                .iter()
                .filter(|s| s.artist.as_ref().map_or(false, |a| *a == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_DATE_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_DATE_PL_PREFIX, "");
            self.get_all_songs_in_library()
                .iter()
                .filter(|s| s.date.as_ref().map_or(false, |d| *d == pl_name))
                .take(100)
                .cloned()
                .collect()
        } else if playlist_id.starts_with(BY_FOLDER_PL_PREFIX) {
            let pl_name = playlist_id.replace(BY_FOLDER_PL_PREFIX, "");
            self.get_all_songs_in_library()
                .iter()
                .filter(|s| {
                    s.file
                        .split('/')
                        .nth(BY_FOLDER_DEPTH)
                        .map_or(false, |d| *d == pl_name)
                })
                .take(100)
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }

    fn load_song_in_queue(&self, song_id: &str) {
        self.clear_queue();
        self.add_song_in_queue(song_id);
        self.play_from_current_queue_song();
    }

    fn add_song_in_queue(&self, song_id: &str) {
        self.execute_mpd_command(
            format!("add \"{song_id}\"").as_str(),
            |reader| -> Option<String> {
                let mut out = String::new();
                reader.read_line(&mut out).expect("Failed to read response");
                debug!("Response line {}", out);
                None
            },
        );
    }

    fn clear_queue(&self) {
        _ = self.mpd_client.lock().unwrap().clear();
    }

    fn save_queue_as_playlist(&self, playlist_name: &str) {
        _ = self.mpd_client.lock().unwrap().save(playlist_name);
    }

    fn rescan_metadata(&self) {
        _ = self.mpd_client.lock().unwrap().rescan();
    }
}

fn map_song(song: &MpdSong) -> Song {
    debug!("Song is {:?}", song);
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
        info!("Shutting down MPD player!");
        self.stop_current_song();
        _ = self.mpd_client.lock().unwrap().close();
        info!("MPD player shutdown finished!");
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
    connection.map_or_else(
        || {
            Err(anyhow::format_err!(
                "Failed connect to to MPD server! [{}]",
                last_error.unwrap()
            ))
        },
        Ok,
    )
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
                        debug!("Unmatched:|{}|", song_buffer);
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
