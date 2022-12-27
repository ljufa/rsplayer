use std::sync::Arc;

use anyhow::Result;
use api_models::common::PlayerType;
use api_models::settings::Settings;

use crate::{
    mpd::MpdPlayerClient,
    rsp::RsPlayer,
    spotify::SpotifyPlayerClient,
    Player,
};
use mockall_double::double;
use rsplayer_config::MutArcConfiguration;
#[double]
use rsplayer_metadata::metadata::MetadataService;
use rspotify::sync::Mutex;

pub type MutArcPlayerService = Arc<Mutex<PlayerService>>;

pub struct PlayerService {
    player: Box<dyn Player + Send>,
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

    pub fn get_current_player(&mut self) -> &mut Box<dyn Player + Send> {
        &mut self.player
    }

    #[allow(unreachable_patterns)]
    fn create_player(
        settings: &Settings,
        metadata_service: Arc<MetadataService>,
    ) -> Result<Box<dyn Player + Send>> {
        match &settings.active_player {
            PlayerType::SPF => {
                let mut sp = SpotifyPlayerClient::new(&settings.spotify_settings)?;
                sp.start_device(&settings.alsa_settings.device_name)?;
                sp.transfer_playback_to_device()?;
                sp.play_current_song();
                Ok(Box::new(sp))
            }
            PlayerType::MPD => {
                let mut mpd = MpdPlayerClient::new(&settings.mpd_settings)?;
                mpd.ensure_mpd_server_configuration(
                    &settings.alsa_settings.device_name,
                    &settings.metadata_settings.music_directory,
                )?;
                Ok(Box::new(mpd))
            }
            PlayerType::RSP => {
                let rsp = RsPlayer::new(metadata_service);
                Ok(Box::new(rsp))
            }
            _ => panic!("Unknown type"),
        }
    }
}
