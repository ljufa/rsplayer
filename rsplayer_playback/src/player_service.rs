use std::sync::Arc;

use anyhow::Result;
use api_models::common::PlayerType;
use api_models::settings::Settings;

use crate::{mpd::MpdPlayerClient, rsp::RsPlayer, spotify::SpotifyPlayerClient, Player};
use mockall_double::double;
use rsplayer_config::MutArcConfiguration;
#[double]
use rsplayer_metadata::metadata::MetadataService;

pub type ArcPlayerService = Arc<PlayerService>;

pub struct PlayerService {
    player: Box<dyn Player + Send + Sync>,
}

impl PlayerService {

    pub fn new(
        config: &MutArcConfiguration,
        metadata_service: Arc<MetadataService>,
    ) -> Result<Self> {
        let settings = config.lock().unwrap().get_settings();
        Ok(PlayerService {
            player: Self::create_player(&settings, metadata_service)?,
        })
    }

    #[allow(clippy::borrowed_box)]
    pub fn get_current_player(&self) -> &Box<dyn Player + Send + Sync> {
        &self.player
    }

    #[allow(unreachable_patterns)]
    fn create_player(
        settings: &Settings,
        metadata_service: Arc<MetadataService>,
    ) -> Result<Box<dyn Player + Send + Sync>> {
        match &settings.active_player {
            PlayerType::SPF => {
                let mut sp = SpotifyPlayerClient::new(&settings.spotify_settings)?;
                sp.start_device(&settings.alsa_settings.output_device.name)?;
                let device = sp.transfer_playback_to_device()?;
                sp.set_device(device);
                Ok(Box::new(sp))
            }
            PlayerType::MPD => {
                let mut mpd = MpdPlayerClient::new(&settings.mpd_settings)?;
                mpd.ensure_mpd_server_configuration(
                    &settings.alsa_settings.output_device.name,
                    &settings.metadata_settings.music_directory,
                )?;
                Ok(Box::new(mpd))
            }
            PlayerType::RSP => {
                let rsp =
                    RsPlayer::new(metadata_service, settings);
                Ok(Box::new(rsp))
            }
            PlayerType::LMS => panic!("Unsupported type"),
        }
    }
}
