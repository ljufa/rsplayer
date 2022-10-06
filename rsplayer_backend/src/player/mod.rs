use api_models::player::Song;
use api_models::playlist::{Category, DynamicPlaylistsPage, Playlists};
use api_models::state::{PlayerInfo, PlayingContext, PlayingContextQuery, SongProgress};

pub mod mpd;

pub mod player_service;
pub mod spotify;
pub mod spotify_oauth;

pub trait Player {
    fn play(&mut self);
    fn pause(&mut self);
    fn next_track(&mut self);
    fn prev_track(&mut self);
    fn stop(&mut self);
    fn shutdown(&mut self);
    fn rewind(&mut self, seconds: i8);
    fn random_toggle(&mut self);
    fn load_playlist(&mut self, pl_id: String);
    fn load_album(&mut self, album_id: String);
    fn load_song(&mut self, song_id: String);
    fn add_song_to_queue(&mut self, song_id: String);
    fn play_item(&mut self, id: String);
    fn remove_playlist_item(&mut self, id: String);
    fn get_song_progress(&mut self) -> SongProgress;
    fn get_current_song(&mut self) -> Option<Song>;
    fn get_player_info(&mut self) -> Option<PlayerInfo>;
    fn get_playing_context(&mut self, query: PlayingContextQuery) -> Option<PlayingContext>;
    fn get_playlist_categories(&mut self) -> Vec<Category>;
    fn get_static_playlists(&mut self) -> Playlists;
    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage>;
    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<Song>;
    fn clear_queue(&mut self);
    fn save_queue_as_playlist(&mut self, playlist_name: String);
}
