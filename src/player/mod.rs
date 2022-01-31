use mockall_double::double;
use num_traits::{FromPrimitive, ToPrimitive};
use std::sync::Arc;

#[double]
use crate::audio_device::alsa::AudioCard;
use crate::common::{CommandEvent, PlayerStatus, PlayerType, Result};
use crate::config::Settings;
use crate::player::lms::LogitechMediaServerApi;
use crate::player::mpd::MpdPlayerApi;
use crate::player::spotify::SpotifyPlayerApi;

pub(crate) mod lms;
pub(crate) mod mpd;
pub(crate) mod spotify;

pub trait Player {
    fn play(&mut self) -> Result<CommandEvent>;
    fn pause(&mut self) -> Result<CommandEvent>;
    fn next_track(&mut self) -> Result<CommandEvent>;
    fn prev_track(&mut self) -> Result<CommandEvent>;
    fn stop(&mut self) -> Result<CommandEvent>;
    fn shutdown(&mut self);
    fn rewind(&mut self, seconds: i8) -> Result<CommandEvent>;
    fn get_status(&mut self) -> Option<PlayerStatus>;
}

pub struct PlayerFactory {
    player: Box<dyn Player + Send>,
    settings: Settings,
}

impl PlayerFactory {
    pub fn new(current_player: &PlayerType, settings: Settings) -> Self {
        let plr = PlayerFactory::create_this_or_first_available_player(&settings, current_player);
        PlayerFactory {
            player: plr.expect("No players available at this moment"),
            settings,
        }
    }

    pub fn toggle_player(
        &mut self,
        audio_card: Arc<AudioCard>,
        current_player: &PlayerType,
    ) -> Result<PlayerType> {
        self.player.shutdown();
        // time needed to release alsa device lock for next player
        // todo: stop other players if lock can't be released. i.e. if play started externally.
        audio_card.wait_unlock_audio_dev()?;
        let cpt = PlayerFactory::next_player_type(current_player);
        self.player = PlayerFactory::create_this_or_first_available_player(&self.settings, &cpt)?;
        self.player.play()?;
        Ok(cpt)
    }
    pub fn get_current_player(&mut self) -> &mut Box<dyn Player + Send> {
        &mut self.player
    }

    pub fn switch_to_player(
        &mut self,
        audio_card: Arc<AudioCard>,
        player_type: &PlayerType,
    ) -> Result<PlayerType> {
        self.player.shutdown();
        audio_card.wait_unlock_audio_dev()?;
        self.player = PlayerFactory::create_player(&self.settings, player_type)?;
        self.player.play()?;
        Ok(player_type.clone())
    }

    fn create_player(
        settings: &Settings,
        player_type: &PlayerType,
    ) -> Result<Box<dyn Player + Send>> {
        return match player_type {
            PlayerType::SPF => {
                if let Some(spot_set) = &settings.spotify_settings {
                    Ok(Box::new(SpotifyPlayerApi::new(spot_set)?))
                } else {
                    Err(failure::err_msg("failed"))
                }
            }
            PlayerType::MPD => {
                if let Some(mpd_set) = &settings.mpd_settings {
                    Ok(Box::new(MpdPlayerApi::new(mpd_set)?))
                } else {
                    Err(failure::err_msg("failed"))
                }
            }
            PlayerType::LMS => {
                if let Some(lms_set) = &settings.lms_settings {
                    Ok(Box::new(LogitechMediaServerApi::new(lms_set)?))
                } else {
                    Err(failure::err_msg("failed"))
                }
            }
        };
    }

    fn next_player_type(start_from: &PlayerType) -> PlayerType {
        let curr_id: u8 = ToPrimitive::to_u8(start_from).unwrap();
        let next_player_try = FromPrimitive::from_u8(curr_id + 1);
        let next_player;
        if let Some(pl) = next_player_try {
            next_player = pl;
        } else {
            next_player = FromPrimitive::from_u8(1u8).unwrap();
        }
        next_player
    }

    fn create_this_or_first_available_player(
        settings: &Settings,
        start_from: &PlayerType,
    ) -> Result<Box<dyn Player + Send>> {
        let mut tries = 0;
        let mut cpt = start_from.clone();
        let mut cpl = PlayerFactory::create_player(settings, &cpt);
        while cpl.is_err() && tries < 3 {
            trace!("Player {:?} creation failed: {:?}", &cpt, cpl.err());
            cpt = PlayerFactory::next_player_type(&cpt);
            cpl = PlayerFactory::create_player(settings, &cpt);
            tries = tries + 1;
        }
        cpl
    }
}
