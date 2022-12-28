use api_models::player::Song;
use api_models::playlist::{Category, DynamicPlaylistsPage, Playlists};
use api_models::state::{PlayerInfo, PlayingContext, PlayingContextQuery, SongProgress};

pub mod mpd;
pub mod player_service;
pub mod rsp;
pub mod spotify;

pub trait Player {
    // Song
    fn play_queue_from_current_song(&mut self);
    fn pause_current_song(&mut self);
    fn play_next_song(&mut self);
    fn play_prev_song(&mut self);
    fn stop_current_song(&mut self);
    fn seek_current_song(&mut self, seconds: i8);
    fn play_song(&mut self, id: String);
    fn get_current_song(&mut self) -> Option<Song>;

    // Queue
    fn load_playlist_in_queue(&mut self, pl_id: String);
    fn load_album_in_queue(&mut self, album_id: String);
    fn load_song_in_queue(&mut self, song_id: String);
    fn remove_song_from_queue(&mut self, id: String);
    fn add_song_in_queue(&mut self, song_id: String);
    fn clear_queue(&mut self);

    // Playlist
    fn get_playlist_categories(&mut self) -> Vec<Category>;
    fn get_static_playlists(&mut self) -> Playlists;
    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage>;
    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<Song>;
    fn save_queue_as_playlist(&mut self, playlist_name: String);

    // Player
    fn get_player_info(&mut self) -> Option<PlayerInfo>;
    fn get_playing_context(&mut self, query: PlayingContextQuery) -> Option<PlayingContext>;
    fn get_song_progress(&mut self) -> SongProgress;
    fn toggle_random_play(&mut self);
    fn shutdown(&mut self);

    // Metadata????
    fn rescan_metadata(&mut self);
}
