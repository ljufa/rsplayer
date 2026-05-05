use std::sync::Arc;

use api_models::player::Song;

use crate::error::RepoResult;

pub trait SongRepository: Send + Sync {
    fn save(&self, song: &Song) -> RepoResult<()>;
    fn delete(&self, id: &str) -> RepoResult<()>;
    fn delete_all(&self);
    fn find_by_id(&self, id: &str) -> Option<Song>;
    fn find_all(&self) -> Vec<Song>;
    fn find_by_key_contains(&self, search_term: &str) -> Vec<(Vec<u8>, Vec<u8>)>;
    fn find_by_key_prefix(&self, prefix: &str) -> Vec<(Vec<u8>, Vec<u8>)>;
    fn find_songs_by_dir_prefix(&self, prefix: &str) -> Vec<Song>;
    fn flush(&self);
}

pub type ArcSongRepository = Arc<dyn SongRepository>;
