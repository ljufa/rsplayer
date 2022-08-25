
use api_models::common::PlayerType;
use api_models::settings::Settings;

use crate::common::{MutArcConfiguration, Result};

#[cfg(feature = "backend_lms")]
use super::lms::LMSPlayerClient;

#[cfg(feature = "backend_mpd")]
use super::mpd::MpdPlayerClient;

use super::{spotify::SpotifyPlayerClient, Player};

pub struct PlayerService {
    player: Box<dyn Player + Send>,
    settings: Settings,
}

impl PlayerService {
    pub fn new(config: MutArcConfiguration) -> Result<Self> {
        let settings = config.lock().unwrap().get_settings();
        Ok(PlayerService {
            player: Self::create_player(&settings)?,
            settings,
        })
    }

    pub fn get_current_player(&mut self) -> &mut Box<dyn Player + Send> {
        &mut self.player
    }

    #[allow(unreachable_patterns)]
    fn create_player(settings: &Settings) -> Result<Box<dyn Player + Send>> {
        match &settings.active_player {
            PlayerType::SPF => {
                let mut sp = SpotifyPlayerClient::new(settings.spotify_settings.clone())?;
                sp.start_device()?;
                sp.transfer_playback_to_device()?;
                sp.play();
                Ok(Box::new(sp))
            }
            #[cfg(feature = "backend_mpd")]
            PlayerType::MPD => Ok(Box::new(MpdPlayerClient::new(&settings.mpd_settings)?)),
            #[cfg(feature = "backend_lms")]
            PlayerType::LMS => Ok(Box::new(LMSPlayerClient::new(&settings.lms_settings)?)),
            _ => panic!("Unknown type"),
        }
    }
}
