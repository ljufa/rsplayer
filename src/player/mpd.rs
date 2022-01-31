use std::{borrow::BorrowMut, process::Child};
use std::{borrow::Cow, time::Duration};

use mpd::{Client, Query, Term};

use crate::{common::PlayerState, player::Player};
use crate::{
    common::{CommandEvent, PlayerStatus, PlayerType, Result, DPLAY_CONFIG_DIR_PATH},
    config::MpdSettings,
};

pub struct MpdPlayerApi {
    mpd_server_process: Child,
    mpd_client: Client,
    mpd_server_url: String,
}

unsafe impl Send for MpdPlayerApi {}

impl MpdPlayerApi {
    pub fn new(mpd_settings: &MpdSettings) -> Result<MpdPlayerApi> {
        Ok(MpdPlayerApi {
            mpd_server_process: start_mpd_server()?,
            mpd_client: create_client(&mpd_settings)?,
            mpd_server_url: mpd_settings.get_server_url(),
        })
    }
    fn try_with_reconnect<F>(
        &mut self,
        command_event: CommandEvent,
        command: F,
    ) -> Result<CommandEvent>
    where
        F: FnMut(&mut Client) -> core::result::Result<(), mpd::error::Error>,
    {
        match self.try_with_reconnect_result(command) {
            Ok(_) => Ok(command_event),
            Err(e) => Err(e),
        }
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

impl Player for MpdPlayerApi {
    fn play(&mut self) -> Result<CommandEvent> {
        self.try_with_reconnect(CommandEvent::Playing, |client| client.play())
    }

    fn pause(&mut self) -> Result<CommandEvent> {
        self.try_with_reconnect(CommandEvent::Paused, |client| client.pause(true))
    }

    fn next_track(&mut self) -> Result<CommandEvent> {
        self.try_with_reconnect(CommandEvent::SwitchedToNextTrack, |client| client.next())
    }

    fn prev_track(&mut self) -> Result<CommandEvent> {
        self.try_with_reconnect(CommandEvent::SwitchedToPrevTrack, |client| client.prev())
    }

    fn stop(&mut self) -> Result<CommandEvent> {
        self.try_with_reconnect(CommandEvent::Stopped, |client| client.stop())
    }

    fn rewind(&mut self, seconds: i8) -> Result<CommandEvent> {
        let result = self.try_with_reconnect_result(|client| client.status());
        if let Ok(status) = result {
            //todo: implement protection against going of the range
            let position = status.elapsed.unwrap().num_seconds() + seconds as i64;
            self.mpd_client.rewind(position)?;
        }
        Ok(CommandEvent::Playing)
    }

    fn get_status(&mut self) -> Option<PlayerStatus> {
        let result = self.try_with_reconnect_result(|client| client.currentsong());
        let song = result.unwrap_or_else(|_| return None);
        if song.is_none() {
            warn!("Mpd Song is None");
            return None;
        }
        let song = song.unwrap();
        // debug!("Song is {:?}", song);
        let status = self.try_with_reconnect_result(|client| client.status());
        // debug!("Mpd Status is {:?}", status);
        if status.is_err() {
            error!("Error while getting mpd status {:?}", status);
            return None;
        }

        let status = status.unwrap();
        let mut album: Option<String> = None;
        if song.tags.contains_key("Album") {
            album = Some(song.tags["Album"].clone());
        }
        let mut artist: Option<String> = None;
        if song.tags.contains_key("Artist") {
            artist = Some(song.tags["Artist"].clone());
        }
        let mut genre: Option<String> = None;
        if song.tags.contains_key("Genre") {
            genre = Some(song.tags["Genre"].clone());
        }
        let mut date: Option<String> = None;
        if song.tags.contains_key("Date") {
            date = Some(song.tags["Date"].clone());
        }
        Some(PlayerStatus {
            filename: Some(song.file),
            name: song.name,
            album,
            artist,
            genre,
            date,
            audio_format_bit: status.audio.map(|f| f.bits),
            audio_format_rate: status.audio.map(|f| f.rate),
            audio_format_channels: status.audio.map(|f| f.chans as u32),
            random: Some(status.random),
            state: Some(convert_state(status.state)),
            title: song.title,
            uri: None,
            time: None,
            // time: status
            // .time
            // .map_or(None, |f| Some((f.0.to_string(), f.1.to_string()))),
        })
    }

    fn shutdown(&mut self) {
        info!("Shutting down MPD player!");
        self.stop();
        self.mpd_client.close();
        self.mpd_server_process.kill();
    }
}
fn convert_state(mpd_state: mpd::status::State) -> PlayerState {
    match mpd_state {
        mpd::State::Stop => PlayerState::STOPPED,
        mpd::State::Play => PlayerState::PLAYING,
        mpd::State::Pause => PlayerState::PAUSED,
    }
}
impl Drop for MpdPlayerApi {
    fn drop(&mut self) {
        self.shutdown()
    }
}

fn start_mpd_server() -> Result<Child> {
    info!("Starting mpd server process!");
    let child = std::process::Command::new("/usr/bin/mpd")
        .arg("--no-daemon")
        .arg("-v")
        .arg(format!("{}mpd.conf", DPLAY_CONFIG_DIR_PATH))
        .spawn();
    match child {
        Ok(c) => Ok(c),
        Err(e) => Err(failure::format_err!(
            "Can't start mpd process. Error: {}",
            e
        )),
    }
}
fn create_client(mpd_settings: &MpdSettings) -> Result<Client> {
    let mut tries = 0;
    let mut connection = None;
    while tries < 5 {
        tries += 1;
        info!("Trying to connect to MPD server. Attempt no: {}", tries);
        let conn = Client::connect(mpd_settings.get_server_url().as_str());
        if let Ok(mut conn) = conn {
            if conn.queue()?.is_empty() {
                info!("Mpd playlist is empty, creating default one.");
                conn.findadd(&Query::new().and(Term::Tag(Cow::from("date")), "2020"))?;
                conn.random(true)?;
            }
            connection = Some(conn);
            break;
        } else {
            std::thread::sleep(Duration::from_secs(1))
        }
    }
    match connection {
        Some(c) => Ok(c),
        None => Err(failure::err_msg("Can't connecto to MPD server!")),
    }
}
