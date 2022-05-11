use api_models::player::*;
use api_models::playlist::Playlist;
use api_models::state::PlayerInfo;

#[cfg(feature = "backend_lms")]
pub(crate) mod lms;
#[cfg(feature = "backend_mpd")]
pub(crate) mod mpd;

pub(crate) mod player_service;
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
    fn load_playlist(&mut self, pl_id: String);
    fn get_queue_items(&mut self) -> Vec<Song>;
    fn get_playlist_items(&mut self, playlist_name: String) -> Vec<Song>;
    fn play_at(&mut self, position: u32);
}
