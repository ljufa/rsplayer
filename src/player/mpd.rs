use std::borrow::BorrowMut;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Error, ErrorKind, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::time::Duration;

use api_models::player::*;
use api_models::playlist::Playlist;
use api_models::settings::*;
use mpd::{Client, Song as MpdSong};
use num_traits::ToPrimitive;

use crate::common::Result;

use super::Player;

pub struct MpdPlayerClient {
    mpd_client: Client,
    mpd_server_url: String,
}

unsafe impl Send for MpdPlayerClient {}

impl MpdPlayerClient {
    pub fn new(mpd_settings: &MpdSettings) -> Result<MpdPlayerClient> {
        if !mpd_settings.enabled {
            return Err(failure::err_msg("MPD player integration is disabled."));
        }
        Ok(MpdPlayerClient {
            mpd_client: create_client(mpd_settings)?,
            mpd_server_url: mpd_settings.get_server_url(),
        })
    }
    fn try_with_reconnect_result<F, R>(&mut self, mut command: F) -> Result<R>
    where
        F: FnMut(&mut Client) -> mpd::error::Result<R>,
    {
        let mut result = command(self.mpd_client.borrow_mut());
        if result.is_err() {
            match Client::connect(self.mpd_server_url.as_str()) {
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
}

impl Player for MpdPlayerClient {
    fn play(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.play());
    }
    fn play_at(&mut self, position: u32) {
        _ = self.try_with_reconnect_result(|client| client.switch(position));
    }

    fn pause(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.pause(true));
    }

    fn next_track(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.next());
    }

    fn prev_track(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.prev());
    }

    fn stop(&mut self) {
        _ = self.try_with_reconnect_result(|client| client.stop());
    }

    fn shutdown(&mut self) {
        info!("Shutting down MPD player!");
        _ = self.stop();
        _ = self.mpd_client.close();
        info!("MPD player shutdown finished!");
    }

    fn rewind(&mut self, seconds: i8) {
        let result = self.try_with_reconnect_result(|client| client.status());
        if let Ok(status) = result {
            //todo: implement protection against going of the range
            let position = status.elapsed.unwrap().num_seconds() + seconds as i64;
            _ = self.mpd_client.rewind(position);
        };
    }

    fn get_current_song(&mut self) -> Option<Song> {
        let result = self.try_with_reconnect_result(|client| client.currentsong());
        let song = result.unwrap_or(None);
        Some(mpd_song_to_song(&song.unwrap()))
    }
    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        let status = self.try_with_reconnect_result(|client| client.status());
        trace!("Mpd Status is {:?}", status);
        if let Ok(status) = status {
            Some(PlayerInfo {
                audio_format_bit: status.audio.map(|f| f.bits),
                audio_format_rate: status.audio.map(|f| f.rate),
                audio_format_channels: status.audio.map(|f| f.chans as u32),
                random: Some(status.random),
                state: Some(convert_state(status.state)),
                time: status.time.map_or((Duration::ZERO, Duration::ZERO), |t| {
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
                }),
            })
        } else {
            error!("Error while getting mpd status {:?}", status);
            None
        }
    }

    fn random_toggle(&mut self) {
        let status = self.try_with_reconnect_result(|client| client.status());
        if let Ok(status) = status {
            _ = self.mpd_client.random(!status.random);
        }
    }

    fn get_playlists(&mut self) -> Vec<Playlist> {
        let pls = self
            .try_with_reconnect_result(|client| client.playlists())
            .unwrap_or_default();
        pls.into_iter().map(|p| Playlist { name: p.name }).collect()
    }

    fn load_playlist(&mut self, pl_name: String) {
        let r = self.try_with_reconnect_result(|client| {
            _ = client.clear();
            client.load(pl_name.clone(), ..)
        });
        info!("Load pl result: {:?}", r);
    }

    fn get_queue_items(&mut self) -> Vec<Song> {
        let r = send_command("playlistinfo").unwrap_or_default();
        r.into_iter().take(50).collect()
    }

    fn get_playlist_items(&mut self, playlist_name: String) -> Vec<Song> {
        let r = send_command(format!("listplaylistinfo {}", playlist_name).as_str())
            .unwrap_or_default();
        r.into_iter().take(100).collect()
    }
}

fn mpd_song_to_song(song: &MpdSong) -> Song {
    trace!("Song is {:?}", song);

    Song {
        file: song.file.clone(),
        title: song.title.clone(),
        position: song.place.map(|p| p.id.0),
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
        time: tag_to_value(song, "Time"),
        track: tag_to_value(song, "Track"),
        tags: HashMap::new(),
        uri: None,
    }
}
fn tag_to_value(song: &MpdSong, key: &str) -> Option<String> {
    song.tags.iter().find(|t| t.0 == key).map(|kv| kv.1.clone())
}

fn convert_state(mpd_state: mpd::status::State) -> PlayerState {
    match mpd_state {
        mpd::State::Stop => PlayerState::STOPPED,
        mpd::State::Play => PlayerState::PLAYING,
        mpd::State::Pause => PlayerState::PAUSED,
    }
}
impl Drop for MpdPlayerClient {
    fn drop(&mut self) {
        self.shutdown()
    }
}
fn create_client(mpd_settings: &MpdSettings) -> Result<Client> {
    let mut tries = 0;
    let mut connection = None;

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
                // std::thread::sleep(Duration::from_secs(1))
            }
        }
    }
    match connection {
        Some(c) => Ok(c),
        None => Err(failure::err_msg("Can't connecto to MPD server!")),
    }
}

fn send_command(command: &str) -> std::io::Result<Vec<Song>> {
    let mut full_cmd = String::new();
    full_cmd.push_str(command);
    full_cmd.push('\n');

    let mut client = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 5, 59)), 6677),
        Duration::from_secs(2),
    )
    .unwrap();

    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("Failed to set read timeout");
    client
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("Failed to set write timeout");
    client.write_all(full_cmd.as_bytes())?;

    let mut conn = BufReader::new(&mut client);

    let mut file_buffer = String::new();
    // skip header lines
    conn.read_line(&mut file_buffer).unwrap_or_default();
    if !file_buffer.starts_with("OK MPD") {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "MPD protocol not detected",
        ));
    }
    for _ in 1..5 {
        file_buffer.clear();
        conn.read_line(&mut file_buffer).unwrap_or_default();
        if file_buffer.starts_with("ACK") {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("wrong command parameters: {}", file_buffer),
            ));
        }
        if file_buffer.starts_with("file") {
            break;
        }
    }
    let mut result = Vec::<Song>::new();

    loop {
        if file_buffer.starts_with("file:") {
            let mut p = Song {
                file: to_opt_string(file_buffer.split_once(':').unwrap_or_default().1)
                    .unwrap_or_default(),
                ..Default::default()
            };
            'song: loop {
                let mut song_buf = String::new();
                conn.read_line(&mut song_buf).unwrap_or_default();

                // end of response
                if song_buf == "OK\n" {
                    file_buffer.clear();
                    break 'song;
                }
                if !song_buf.contains(':') {
                    continue;
                }

                let pair = song_buf.split_once(':').unwrap_or_default();
                let key = pair.0;
                let value = pair.1;
                match key {
                    "Artist" => p.artist = to_opt_string(value),
                    "Title" => p.title = to_opt_string(value),
                    "Genre" => p.genre = to_opt_string(value),
                    "Album" => p.album = to_opt_string(value),
                    "Date" => p.date = to_opt_string(value),
                    "Track" => p.track = to_opt_string(value),
                    "Time" => p.time = to_opt_string(value),
                    "Pos" => p.position = to_opt_u32(value),
                    "Last-Modified" => p.last_modified = to_opt_string(value),
                    "Performer" => p.performer = to_opt_string(value),
                    "Composer" => p.composer = to_opt_string(value),
                    "AlbumArtist" => p.album_artist = to_opt_string(value),
                    "Disc" => p.disc = to_opt_string(value),
                    "Label" => p.label = to_opt_string(value),
                    "Range" | "Id" | "duration" => {}
                    "file" => {
                        file_buffer = song_buf;
                        break 'song;
                    }
                    &_ => {
                        debug!("Unmatch:|{}|", song_buf);
                        p.tags.insert(
                            String::from_str(key).unwrap(),
                            to_opt_string(value).unwrap(),
                        );
                    }
                }
            }
            result.push(p);
        } else {
            break;
        }
    }
    Ok(result)
}
fn to_opt_string(value: &str) -> Option<String> {
    String::from_str(value.replace('\"', "").trim()).ok()
}
fn to_opt_u32(value: &str) -> Option<u32> {
    to_opt_string(value).map(|tr| tr.parse::<u32>().unwrap_or_default())
}
#[cfg(test)]
mod test {

    use super::send_command;

    #[test]
    fn test_client() {
        let songs = send_command("currentsong").unwrap();
        assert_eq!(songs.len(), 1);
    }

    #[test]
    fn test_trim() {
        assert_eq!("\" Artist\n".replace('\"', "").trim(), "Artist");
    }
}
