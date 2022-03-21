use std::sync::Arc;

use crate::audio_device::alsa::AudioCard;
use crate::common::Result;
#[cfg(feature = "backend_lms")]
use crate::player::lms::LogitechMediaServerApi;
#[cfg(feature = "backend_mpd")]
use crate::player::mpd::MpdPlayerApi;
#[cfg(feature = "backend_spotify")]
use crate::player::spotify::SpotifyPlayerApi;

use api_models::player::*;
use api_models::settings::*;

#[cfg(feature = "backend_lms")]
pub(crate) mod lms;
#[cfg(feature = "backend_mpd")]
pub(crate) mod mpd;
#[cfg(feature = "backend_spotify")]
pub(crate) mod spotify;

pub trait Player {
    fn play(&mut self) -> Result<StatusChangeEvent>;
    fn pause(&mut self) -> Result<StatusChangeEvent>;
    fn next_track(&mut self) -> Result<StatusChangeEvent>;
    fn prev_track(&mut self) -> Result<StatusChangeEvent>;
    fn stop(&mut self) -> Result<StatusChangeEvent>;
    fn shutdown(&mut self);
    fn rewind(&mut self, seconds: i8) -> Result<StatusChangeEvent>;
    fn get_current_track_info(&mut self) -> Option<CurrentTrackInfo>;
    fn get_player_info(&mut self) -> Option<PlayerInfo>;
    fn random_toggle(&mut self);
}

pub struct PlayerFactory {
    player: Box<dyn Player + Send>,
    settings: Settings,
}

impl PlayerFactory {
    pub fn new(current_player: &PlayerType, settings: Settings) -> Result<Self> {
        Ok(PlayerFactory {
            player: PlayerFactory::create_player(&settings, current_player)?,
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
        let new_player = PlayerFactory::create_player(&self.settings, player_type)?;
        self.player = new_player;
        self.player.play()?;
        Ok(player_type.clone())
    }

    fn create_player(
        settings: &Settings,
        player_type: &PlayerType,
    ) -> Result<Box<dyn Player + Send>> {
        return match player_type {
            #[cfg(feature = "backend_spotify")]
            PlayerType::SPF => Ok(Box::new(SpotifyPlayerApi::new(&settings.spotify_settings)?)),
            #[cfg(feature = "backend_mpd")]
            PlayerType::MPD => Ok(Box::new(MpdPlayerApi::new(&settings.mpd_settings)?)),
            #[cfg(feature = "backend_lms")]
            PlayerType::LMS => Ok(Box::new(LogitechMediaServerApi::new(
                &settings.lms_settings,
            )?)),
            _ => panic!("Unknown type"),
        };
    }
}
