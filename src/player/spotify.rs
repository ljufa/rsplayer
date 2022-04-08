use std::process::Child;
use std::time::Duration;

use api_models::player::*;
use api_models::playlist::Playlist;
use api_models::settings::*;
use failure::err_msg;
use rspotify::clients::OAuthClient;

use crate::common::Result;
use crate::config::Configuration;
use crate::player::Player;
use log::info;

use super::spotify_oauth::SpotifyOauth;

pub struct SpotifyPlayerClient {
    librespot_process: Option<Child>,
    client: SpotifyOauth,
    device_id: Option<String>,
}

unsafe impl Send for SpotifyPlayerClient {}

impl SpotifyPlayerClient {
    pub fn new(settings: SpotifySettings) -> Result<SpotifyPlayerClient> {
        if !settings.enabled {
            return Err(err_msg("Spotify integration is disabled."));
        }
        let mut client = SpotifyOauth::new(settings);
        if !client.is_token_present()? {
            return Err(err_msg(
                "Spotify token not found, please complete configuration",
            ));
        }
        Ok(SpotifyPlayerClient {
            client,
            librespot_process: None,
            device_id: None,
        })
    }

    pub fn start_device(&mut self) -> Result<()> {
        self.librespot_process = Some(start_librespot(&self.client.settings)?);
        Ok(())
    }

    pub fn transfer_playback_to_device(&mut self) -> Result<()> {
        let mut dev = "".to_string();
        let mut tries = 0;
        while tries < 5 {
            for d in self.client.client.device()? {
                if d.name.contains(self.client.settings.device_name.as_str()) {
                    let device_id = d.id.as_ref();
                    if device_id.is_some() && !d.is_active {
                        self.client
                            .client
                            .transfer_playback(device_id.unwrap().as_str(), Some(false))?;
                    }
                    dev = device_id.unwrap().clone();
                }
            }
            if dev.is_empty() {
                tries += 1;
            } else {
                break;
            }
        }
        if dev.is_empty() {
            return Err(err_msg("Device not found!"));
        }
        info!("Spotify client created sucessfully!");
        self.device_id = Some(dev);
        Ok(())
    }
}

impl Drop for SpotifyPlayerClient {
    fn drop(&mut self) {
        self.shutdown()
    }
}

impl Player for SpotifyPlayerClient {
    fn play(&mut self) {
        _ = self
            .client
            .client
            .resume_playback(self.device_id.as_deref(), None);
    }

    fn pause(&mut self) {
        _ = self.client.client.pause_playback(self.device_id.as_deref());
    }
    fn next_track(&mut self) {
        _ = self.client.client.next_track(self.device_id.as_deref());
    }
    fn prev_track(&mut self) {
        _ = self.client.client.previous_track(self.device_id.as_deref());
    }
    fn stop(&mut self) {
        _ = self.client.client.pause_playback(self.device_id.as_deref());
    }

    fn shutdown(&mut self) {
        info!("Shutting down Spotify player!");
        if self.device_id.is_some() {
            _ = self.stop();
        }
        _ = self.librespot_process.as_mut().unwrap().kill();
    }

    fn rewind(&mut self, _seconds: i8) {}

    fn get_current_song(&mut self) -> Option<Song> {
        if let Some(playing_item) = self.client.client.current_user_playing_item().unwrap() {
            let mut track = playing_item.item.unwrap();
            match &mut track {
                rspotify::model::PlayableItem::Track(track) => Some(Song {
                    album: Some(track.album.name.clone()),
                    artist: track.artists.first().map(|a| a.name.clone()),
                    genre: None,
                    date: track.album.release_date.clone(),
                    file: track.href.as_ref().map_or("".to_string(), |u| u.clone()),
                    title: Some(track.name.clone()),
                    time: Some(track.duration.as_secs().to_string()),
                    uri: track.album.images.first().map(|i| i.url.clone()),
                    ..Default::default()
                }),
                rspotify::model::PlayableItem::Episode(_) => None,
            }
        } else {
            None
        }
    }

    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        if let Ok(Some(c)) = self.client.client.current_playback(None, None::<&[_]>) {
            Some(PlayerInfo {
                time: c
                    .progress
                    .map_or((Duration::ZERO, Duration::ZERO), |f| (Duration::ZERO, f)),
                random: Some(c.shuffle_state),
                state: if c.is_playing {
                    Some(PlayerState::PLAYING)
                } else {
                    Some(PlayerState::PAUSED)
                },
                ..Default::default()
            })
        } else {
            None
        }
    }

    fn random_toggle(&mut self) {}

    fn get_playlists(&mut self) -> Vec<Playlist> {
        vec![]
    }

    fn load_playlist(&mut self, _pl_name: String) {}

    fn get_queue_items(&mut self) -> Vec<Song> {
        vec![]
    }

    fn get_playlist_items(&mut self, _playlist_name: String) -> Vec<Song> {
        vec![]
    }

    fn play_at(&mut self, _position: u32) {}
}

fn start_librespot(settings: &SpotifySettings) -> Result<Child> {
    info!("Starting librespot process");
    let child = std::process::Command::new(Configuration::get_librespot_path())
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
        .arg("--format")
        .arg("F32")
        .arg("--initial-volume")
        .arg("100")
        .arg("--verbose")
        .spawn();
    match child {
        Ok(c) => Ok(c),
        Err(e) => Err(failure::format_err!(
            "Can't start librespot process. Error: {}",
            e
        )),
    }
}
