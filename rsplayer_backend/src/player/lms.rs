use std::net::TcpStream;
use std::{
    io::{BufRead, BufReader, Write},
    process::Child,
};

use crate::common::Result;
use crate::config::Configuration;
use crate::player::Player;
use api_models::player::*;
use api_models::playlist::{Category, DynamicPlaylistsPage, Playlist};
use api_models::settings::*;
use api_models::state::{PlayerInfo, StateChangeEvent::*};
use api_models::state::{SongProgress, StateChangeEvent};

// https://github.com/elParaguayo/LMS-CLI-Documentation/blob/master/LMS-CLI.md

pub struct LMSPlayerClient {
    squeeze_player_process: Child,
    client: TcpStream,
    cli_server_url: String,
}

impl LMSPlayerClient {
    pub fn new(settings: &LmsSettings) -> Result<LMSPlayerClient> {
        if !settings.enabled {
            return Err(failure::err_msg("LMS player integration is not enabled."));
        }
        let mut p = LMSPlayerClient {
            squeeze_player_process: start_squeezelite(settings)?,
            client: TcpStream::connect(settings.get_cli_url())?,
            cli_server_url: settings.get_cli_url(),
        };
        let mut num_tracks = String::from("0");
        let mut tries = 0;

        while (num_tracks.is_empty() || num_tracks == "0") && tries < 5 {
            tries += 1;
            debug!(
                "Attempting to connect to LMS. Attempt = {}, Num of tracks {}",
                tries, num_tracks
            );
            if let Ok(r) = p.send_command_with_response("playlist tracks ?") {
                num_tracks = r.clone();
                if num_tracks.trim().is_empty() || num_tracks == "0" {
                    info!("LMS playlist is empty, creating random tracks list.");
                    p.send_command("randomplay tracks", Playing)?;
                    // std::thread::sleep(std::time::Duration::from_secs(1));
                } else {
                    debug!(
                        "Number of tracks in playlist higher that zero : {}",
                        num_tracks
                    );
                }
            }
        }
        Ok(p)
    }

    fn send_command(
        &mut self,
        command: &'static str,
        event: StateChangeEvent,
    ) -> Result<StateChangeEvent> {
        self.send_command_with_response(command)?;
        Ok(event)
    }

    fn send_command_with_response(&mut self, command: &'static str) -> Result<String> {
        // write request
        let mut full_cmd = String::new();
        full_cmd.push(' ');
        full_cmd.push_str(command);
        full_cmd.push('\n');
        // fixme: izgleda da unwrap uvek vraca 0 u slucaju greske, bolje proveriti na oba
        let bytes_sent = self.client.write(full_cmd.as_bytes()).unwrap_or_else(|_| {
            trace!("First attempt failed");
            if let Ok(s) = TcpStream::connect(self.cli_server_url.as_str()) {
                self.client = s;
                match self.client.write(full_cmd.as_bytes()) {
                    Ok(res) => return res,
                    Err(_) => {
                        trace!("Second attempt failed");
                        return 0;
                    }
                };
            }
            0
        });
        if bytes_sent == 0 {
            return Err(failure::err_msg("Unable to send request to LMS server!"));
        }
        // read response
        let mut buffer = String::new();
        let mut conn = BufReader::new(&mut self.client);
        conn.read_line(&mut buffer).expect("unable to read");
        self.client.flush()?;

        let skip = 27 + command.len();
        let decoded: String = buffer
            .chars()
            .skip(skip)
            .take(buffer.len() - skip - 1)
            .collect();
        let decoded: String = url::form_urlencoded::parse(decoded.as_bytes())
            .map(|(key, val)| [key, val].concat())
            .collect();
        // trace!("Lms server response is {}", &decoded);
        Ok(decoded)
    }
}
impl Player for LMSPlayerClient {
    fn play(&mut self) {
        self.send_command("play", Playing);
    }

    fn pause(&mut self) {
        self.send_command("pause", Paused);
    }
    fn next_track(&mut self) {
        self.send_command("playlist index +1", SwitchedToNextTrack);
    }
    fn prev_track(&mut self) {
        self.send_command("playlist index -1", SwitchedToPrevTrack);
    }

    fn stop(&mut self) {
        self.send_command("stop", Stopped);
    }

    fn shutdown(&mut self) {
        info!("Shutting down LMS player!");
        _ = self.stop();
        _ = self.client.shutdown(std::net::Shutdown::Both);
        _ = self.squeeze_player_process.kill();
    }

    fn rewind(&mut self, _seconds: i8) {}

    fn random_toggle(&mut self) {}

    fn load_playlist(&mut self, _pl_name: String) {
        todo!()
    }

    fn load_album(&mut self, album_id: String) {
        todo!()
    }

    fn play_item(&mut self, id: String) {
        todo!()
    }

    fn remove_playlist_item(&mut self, id: String) {
        todo!()
    }

    fn get_song_progress(&mut self) -> SongProgress {
        todo!()
    }

    fn get_current_song(&mut self) -> Option<Song> {
        let artist = self
            .send_command_with_response("artist ?")
            .map_or(None, |r| if !r.is_empty() { Some(r) } else { None });
        let title = self
            .send_command_with_response("current_title ?")
            .map_or(None, |r| if !r.is_empty() { Some(r) } else { None });
        let album = self
            .send_command_with_response("album ?")
            .map_or(None, |r| if !r.is_empty() { Some(r) } else { None });
        let genre = self
            .send_command_with_response("genre ?")
            .map_or(None, |r| if !r.is_empty() { Some(r) } else { None });

        let path = self.send_command_with_response("path ?").map_or(None, |r| {
            if !r.is_empty() {
                Some(r)
            } else {
                None
            }
        });

        Some(Song {
            album,
            artist,
            genre,
            date: None,
            file: path.unwrap_or_default(),
            title,
            ..Default::default()
        })
    }

    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        None
    }

    fn get_playing_context(
        &mut self,
        include_songs: bool,
    ) -> Option<api_models::state::PlayingContext> {
        todo!()
    }

    fn get_playlist_categories(&mut self) -> Vec<Category> {
        todo!()
    }

    fn get_static_playlists(&mut self) -> Vec<Playlist> {
        todo!()
    }

    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage> {
        todo!()
    }

    fn get_playlist_items(&mut self, _playlist_name: String) -> Vec<Song> {
        todo!()
    }
}

impl Drop for LMSPlayerClient {
    fn drop(&mut self) {
        self.shutdown()
    }
}

fn start_squeezelite(settings: &LmsSettings) -> Result<Child> {
    info!("Starting squeezelite player!");
    let child = std::process::Command::new(Configuration::get_squeezelite_player_path())
        // todo: investigate why localhost:9000 is passed as two args localhost and 9000
        //.arg("-s")
        //.arg(settings.get_player_url())
        // .arg("-U")
        // todo optional check .arg(format!("\"{}\"", settings.alsa_control_device_name.clone()))
        .arg("-o")
        .arg(settings.alsa_pcm_device_name.clone())
        .arg("-D")
        .arg("-C")
        .arg("4")
        .spawn();
    match child {
        Ok(c) => Ok(c),
        Err(e) => Err(failure::format_err!(
            "Can't start squeezelite process. Error: {}",
            e
        )),
    }
}
