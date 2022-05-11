use std::sync::Arc;

use api_models::common::PlayerType;
use api_models::settings::Settings;

use crate::{
    audio_device::audio_service::AudioInterfaceService,
    common::{MutArcConfiguration, Result},
};

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
            player: Self::create_player(&settings, &settings.active_player)?,
            settings,
        })
    }

    pub fn get_current_player(&mut self) -> &mut Box<dyn Player + Send> {
        &mut self.player
    }

    pub fn switch_to_player(
        &mut self,
        audio_card: Arc<AudioInterfaceService>,
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
