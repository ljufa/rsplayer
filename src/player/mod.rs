use std::sync::Arc;

use crate::audio_device::alsa::AudioCard;
use crate::common::Result;
#[cfg(feature = "backend_lms")]
use crate::player::lms::LMSPlayerClient;
#[cfg(feature = "backend_mpd")]
use crate::player::mpd::MpdPlayerClient;
#[cfg(feature = "backend_spotify")]
use crate::player::spotify::SpotifyPlayerClient;

use api_models::player::*;
use api_models::playlist::{Playlist, QueueItem};
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
    fn get_current_track_info(&mut self) -> Option<Track>;
    fn get_player_info(&mut self) -> Option<PlayerInfo>;
    fn random_toggle(&mut self);
    fn get_playlists(&mut self) -> Vec<Playlist>;
    fn load_playlist(&mut self, pl_name: String);

    fn get_queue_items(&mut self) -> Vec<QueueItem>;
}

pub struct PlayerService {
    player: Box<dyn Player + Send>,
    settings: Settings,
}

impl PlayerService {
    pub fn new(current_player: &PlayerType, settings: Settings) -> Result<Self> {
        Ok(PlayerService {
            player: PlayerService::create_player(&settings, current_player)?,
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
        let new_player = PlayerService::create_player(&self.settings, player_type)?;
        self.player = new_player;
        self.player.play()?;
        Ok(*player_type)
    }

    #[allow(unreachable_patterns)]
    fn create_player(
        settings: &Settings,
        player_type: &PlayerType,
    ) -> Result<Box<dyn Player + Send>> {
        return match player_type {
            #[cfg(feature = "backend_spotify")]
            PlayerType::SPF => Ok(Box::new(SpotifyPlayerClient::new(
                &settings.spotify_settings,
            )?)),
            #[cfg(feature = "backend_mpd")]
            PlayerType::MPD => Ok(Box::new(MpdPlayerClient::new(&settings.mpd_settings)?)),
            #[cfg(feature = "backend_lms")]
            PlayerType::LMS => Ok(Box::new(LMSPlayerClient::new(&settings.lms_settings)?)),
            _ => panic!("Unknown type"),
        };
    }
}
