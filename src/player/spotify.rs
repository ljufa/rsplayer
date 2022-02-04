use std::fs;
use std::path::{Path, PathBuf};
use std::{borrow::BorrowMut, process::Child};
use std::{env, time::Duration};

use crate::config::SpotifySettings;
use rspotify::blocking::client::Spotify;
use rspotify::blocking::oauth2::{SpotifyClientCredentials, SpotifyOAuth};
use rspotify::blocking::util::*;
use rspotify::model::offset;

use crate::common::{
    CurrentTrackInfo, PlayerInfo, Result, StatusChangeEvent, DPLAY_CONFIG_DIR_PATH,
};
use crate::player::Player;
use log::{info, trace};

const CACHE_FILE: &str = ".spotify_token_cache";

struct ClientDevice {
    client: Spotify,
    device_id: Option<String>,
}

pub struct SpotifyPlayerApi {
    librespot_process: Child,
    client_device: ClientDevice,
    settings: SpotifySettings,
}
unsafe impl Send for SpotifyPlayerApi {}

impl SpotifyPlayerApi {
    pub fn new(settings: &SpotifySettings) -> Result<SpotifyPlayerApi> {
        Ok(SpotifyPlayerApi {
            librespot_process: start_librespot(settings)?,
            client_device: create_spotify_client(settings)?,
            settings: settings.clone(),
        })
    }
    fn try_with_reconnect<F>(
        &mut self,
        command_event: StatusChangeEvent,
        command: F,
    ) -> Result<StatusChangeEvent>
    where
        F: FnMut(&mut ClientDevice) -> core::result::Result<(), failure::Error>,
    {
        match self.try_with_reconnect_result(command) {
            Ok(_) => Ok(command_event),
            Err(e) => Err(e),
        }
    }

    fn try_with_reconnect_result<F, R>(&mut self, mut command: F) -> Result<R>
    where
        F: FnMut(&mut ClientDevice) -> core::result::Result<R, failure::Error>,
    {
        let mut result = command(&mut self.client_device);
        if let Err(er) = result {
            trace!("First attempt failed with error: {}", er);
            match create_spotify_client(&self.settings) {
                Ok(spot) => {
                    self.client_device = spot;
                    result = command(self.client_device.borrow_mut());
                }
                Err(e) => {
                    trace!("Second attempt failed with error: {}", e);
                    result = Err(e);
                }
            }
        }
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(failure::format_err!(
                "Spotify command failed with error: {}",
                e
            )),
        }
    }
}
impl Drop for SpotifyPlayerApi {
    fn drop(&mut self) {
        self.shutdown()
    }
}
impl Player for SpotifyPlayerApi {
    fn play(&mut self) -> Result<StatusChangeEvent> {
        self.try_with_reconnect_result(|sp| match sp.client.current_user_playing_track() {
            Ok(playing) => match playing {
                Some(pl) => {
                    if !pl.is_playing {
                        let offset = pl
                            .item
                            .as_ref()
                            .map(|it| offset::for_uri(it.uri.clone()).unwrap());

                        let track: Option<String> = pl.item.as_ref().map(|ft| {
                            return ft.uri.clone();
                        });

                        let ctx = pl.context.map(|ct| ct.uri);
                        if ctx.is_some() {
                            sp.client.start_playback(
                                sp.device_id.clone(),
                                ctx,
                                None,
                                offset,
                                pl.progress_ms,
                            )?;
                        } else if let Some(track) = track {
                            sp.client.start_playback(
                                sp.device_id.clone(),
                                None,
                                Some(vec![track]),
                                offset,
                                pl.progress_ms,
                            )?;
                        }
                    }
                    Ok(StatusChangeEvent::Playing)
                }
                None => {
                    let last_played = &sp.client.current_user_recently_played(1)?.items[0]
                        .track
                        .uri;
                    trace!(
                        "Start playing last played song {:?} on dev {:?}",
                        last_played,
                        &sp.device_id
                    );
                    sp.client.start_playback(
                        sp.device_id.clone(),
                        Some(last_played.to_string()),
                        None,
                        None,
                        None,
                    )?;
                    Ok(StatusChangeEvent::Playing)
                }
            },
            Err(e) => Err(e),
        })
    }

    fn pause(&mut self) -> Result<StatusChangeEvent> {
        self.try_with_reconnect(StatusChangeEvent::Paused, |sp| {
            sp.client.pause_playback(sp.device_id.clone())
        })
    }
    fn next_track(&mut self) -> Result<StatusChangeEvent> {
        self.try_with_reconnect(StatusChangeEvent::Paused, |sp| {
            sp.client.next_track(sp.device_id.clone())
        })
    }
    fn prev_track(&mut self) -> Result<StatusChangeEvent> {
        self.try_with_reconnect(StatusChangeEvent::Paused, |sp| {
            sp.client.previous_track(sp.device_id.clone())
        })
    }
    fn stop(&mut self) -> Result<StatusChangeEvent> {
        self.try_with_reconnect(StatusChangeEvent::Paused, |sp| {
            sp.client.pause_playback(sp.device_id.clone())
        })
    }

    fn shutdown(&mut self) {
        info!("Shutting down Spotify player!");
        self.stop();
        self.librespot_process.kill();
    }

    fn rewind(&mut self, _seconds: i8) -> Result<StatusChangeEvent> {
        Ok(StatusChangeEvent::Playing)
    }

    fn get_current_track_info(&mut self) -> Option<CurrentTrackInfo> {
        match self.try_with_reconnect_result(|sp| {
            let playing = sp.client.current_user_playing_track()?;
            if let Some(playing) = playing {
                let mut track = playing.item.unwrap();
                let mut artist = String::new();
                if !track.artists.is_empty() {
                    artist = track.artists.pop().unwrap().name;
                }
                let _durati = track.duration_ms.to_string().clone();
                Ok(CurrentTrackInfo {
                    name: Some(format!("{} - {}", artist, track.name)),
                    album: Some(track.album.name),
                    artist: Some(artist),
                    genre: None,
                    date: track.album.release_date,
                    filename: None,
                    title: Some(track.name.clone()),
                    uri: track.album.images.into_iter().map(|f| f.url).next(),
                })
            } else {
                Err(failure::err_msg("Can't get spotify track info"))
            }
        }) {
            Ok(ps) => Some(ps),
            Err(_) => None,
        }
    }

    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        None
    }

    fn random_toggle(&mut self) {}
}

pub fn auth_manager(settings: &SpotifySettings) -> SpotifyOAuth {
    // Please notice that protocol of redirect_uri, make sure it's http(or https). It will fail if you mix them up.
    SpotifyOAuth::default()
        .client_id(settings.developer_client_id.as_str())
        .client_secret(settings.developer_secret.as_str())
        .redirect_uri(settings.auth_callback_url.as_str())
        .cache_path(cache_path())
        .scope("user-read-currently-playing playlist-modify-private user-read-recently-played user-modify-playback-state user-read-playback-state")
        .build()
}

fn cache_path() -> PathBuf {
    let project_dir_path = env::current_dir().unwrap();
    let mut cache_path = PathBuf::from(project_dir_path);
    cache_path.push(DPLAY_CONFIG_DIR_PATH);
    let cache_dir = cache_path.display().to_string();
    cache_path.push(CACHE_FILE);
    if !Path::new(cache_dir.as_str()).exists() {
        fs::create_dir_all(cache_dir).unwrap();
    }
    cache_path
}

fn create_spotify_client(settings: &SpotifySettings) -> Result<ClientDevice> {
    let token_info = get_token(&mut auth_manager(settings));
    if token_info.is_none() {
        return Err(failure::format_err!("Can't get token info!"));
    }
    let client_credential = SpotifyClientCredentials::default()
        .token_info(token_info.unwrap())
        .build();
    let spot = Spotify::default()
        .client_credentials_manager(client_credential)
        .build();
    let mut dev = "".to_string();
    let mut tries = 0;
    while tries < 5 {
        for d in spot.device()?.devices {
            if d.name.contains(settings.device_name.as_str()) {
                let device_id = &d.id;
                if !d.is_active {
                    spot.transfer_playback(device_id.as_str(), false)?;
                }
                dev = device_id.clone();
            }
        }
        if dev.is_empty() {
            std::thread::sleep(Duration::from_millis(2000));
            tries += 1;
        } else {
            break;
        }
    }
    if dev.is_empty() {
        return Err(failure::err_msg("Device not found!"));
    }
    info!("Spotify client created sucessfully!");

    Ok(ClientDevice {
        client: spot,
        device_id: Some(dev),
    })
}
fn start_librespot(settings: &SpotifySettings) -> Result<Child> {
    info!("Starting librespot process");
    let child = std::process::Command::new(format!("{}librespot", DPLAY_CONFIG_DIR_PATH))
        .arg("--disable-audio-cache")
        .arg("--bitrate")
        .arg(settings.bitrate.to_string())
        .arg("--name")
        .arg(settings.device_name.clone())
        .arg("--backend")
        .arg("alsa")
        .arg("--username")
        .arg(settings.username.clone())
        .arg("--password")
        .arg(settings.password.clone())
        .arg("--device")
        .arg(settings.alsa_device_name.clone())
        .arg("--verbose")
        .arg("--initial-volume")
        .arg("100")
        .spawn();
    match child {
        Ok(c) => Ok(c),
        Err(e) => Err(failure::format_err!(
            "Can't start librespot process. Error: {}",
            e
        )),
    }
}
