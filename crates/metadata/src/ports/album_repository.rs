use std::sync::Arc;

use api_models::{player::Song, playlist::Album};

use crate::error::RepoResult;

pub trait AlbumRepository: Send + Sync {
    fn delete_all(&self);
    fn find_all(&self) -> Vec<Album>;
    fn find_all_album_artists(&self) -> Vec<String>;
    fn find_by_id(&self, album_id: &str) -> Option<Album>;
    fn find_all_sort_by_added_desc(&self, limit: usize) -> Vec<Album>;
    fn find_all_sort_by_released_desc(&self, limit: usize) -> Vec<Album>;
    fn find_all_by_genre(&self, limit_per_genre: usize) -> Vec<(String, Vec<Album>)>;
    fn find_all_by_decade(&self, limit_per_decade: usize) -> Vec<(String, Vec<Album>)>;
    fn find_by_artist(&self, artist: &str) -> Vec<Album>;
    fn update_from_song(&self, song: Song) -> RepoResult<()>;
}

pub type ArcAlbumRepository = Arc<dyn AlbumRepository>;
