use std::sync::Arc;

use crate::audio_device::alsa::AudioCard;
use crate::common::Result;
#[cfg(feature = "backend_lms")]
use crate::player::lms::LMSPlayerClient;
#[cfg(feature = "backend_mpd")]
use crate::player::mpd::MpdPlayerClient;

use crate::player::spotify::SpotifyPlayerClient;

use api_models::player::*;
use api_models::playlist::Playlist;
use api_models::settings::*;

#[cfg(feature = "backend_lms")]
pub(crate) mod lms;
#[cfg(feature = "backend_mpd")]
pub(crate) mod mpd;

pub(crate) mod spotify;
pub(crate) mod spotify_oauth;

pub trait Player {
    fn play(&mut self);
    fn pause(&mut self);
    fn next_track(&mut self);
    fn prev_track(&mut self);
    fn stop(&mut self);
    fn shutdown(&mut self);
    fn rewind(&mut self, seconds: i8);
    fn get_current_song(&mut self) -> Option<Song>;
    fn get_player_info(&mut self) -> Option<PlayerInfo>;
    fn random_toggle(&mut self);
    fn get_playlists(&mut self) -> Vec<Playlist>;
    fn load_playlist(&mut self, pl_name: String);
    fn get_queue_items(&mut self) -> Vec<Song>;
    fn get_playlist_items(&mut self, playlist_name: String) -> Vec<Song>;
    fn play_at(&mut self, position: u32);
}

pub struct PlayerService {
    player: Box<dyn Player + Send>,
    settings: Settings,
}

impl PlayerService {
    pub fn new(current_player: &PlayerType, settings: Settings) -> Result<Self> {
        Ok(PlayerService {
            player: Self::create_player(&settings, current_player)?,
            settings,
        })
    }

    pub fn get_current_player(&mut self) -> &mut Box<dyn Player + Send> {
        &mut self.player
    }

    pub fn switch_to_player(
        &mut self,
        audio_card: Arc<AudioCard>,
        player_type: &PlayerType,
    ) -> Result<PlayerType> {
        let _ = self.player.stop();
        audio_card.wait_unlock_audio_dev()?;
        let new_player = Self::create_player(&self.settings, player_type)?;
        self.player = new_player;
        self.player.play();
        Ok(*player_type)
    }

    #[allow(unreachable_patterns)]
    fn create_player(
        settings: &Settings,
        player_type: &PlayerType,
    ) -> Result<Box<dyn Player + Send>> {
        return match player_type {
            PlayerType::SPF => {
                let mut sp = SpotifyPlayerClient::new(settings.spotify_settings.clone())?;
                sp.start_device()?;
                sp.transfer_playback_to_device()?;
                Ok(Box::new(sp))
            }
            #[cfg(feature = "backend_mpd")]
            PlayerType::MPD => Ok(Box::new(MpdPlayerClient::new(&settings.mpd_settings)?)),
            #[cfg(feature = "backend_lms")]
            PlayerType::LMS => Ok(Box::new(LMSPlayerClient::new(&settings.lms_settings)?)),
            _ => panic!("Unknown type"),
        };
    }
}
