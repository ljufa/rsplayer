use fjall::{Database, Keyspace, KeyspaceCreateOptions};
use log::error;

use api_models::player::Song;

use crate::error::{RepoError, RepoResult};
pub use crate::ports::song_repository::{ArcSongRepository, SongRepository};

pub struct FjallSongRepository {
    pub(crate) songs_db: Keyspace,
}

impl FjallSongRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            songs_db: db
                .keyspace("songs", KeyspaceCreateOptions::default)
                .expect("Failed to open songs keyspace"),
        }
    }

    /// Standalone constructor for tests — opens its own fjall Database.
    pub fn new_standalone(db_path: &str) -> Self {
        let db = Database::builder(db_path).open().expect("Failed to open song db");
        Self {
            songs_db: db
                .keyspace("songs", KeyspaceCreateOptions::default)
                .expect("Failed to open songs keyspace"),
        }
    }
}

impl SongRepository for FjallSongRepository {
    fn save(&self, song: &Song) -> RepoResult<()> {
        if song.file.is_empty() {
            return Err(RepoError::Invalid(
                "refusing to save song with empty file key".to_owned(),
            ));
        }
        self.songs_db
            .insert(&song.file, song.to_json_string_bytes())
            .map_err(|e| RepoError::Storage(format!("save song '{}': {e}", song.file)))
    }

    fn delete(&self, id: &str) -> RepoResult<()> {
        self.songs_db
            .remove(id)
            .map_err(|e| RepoError::Storage(format!("delete song '{id}': {e}")))
    }

    fn delete_all(&self) {
        _ = self.songs_db.clear();
    }

    fn find_by_id(&self, id: &str) -> Option<Song> {
        match self.songs_db.get(id) {
            Ok(Some(v)) => Song::bytes_to_song(&v).or_else(|| {
                error!("song decode failed for key '{id}'");
                None
            }),
            Ok(None) => None,
            Err(e) => {
                error!("song read error for key '{id}': {e}");
                None
            }
        }
    }

    fn find_all(&self) -> Vec<Song> {
        self.songs_db
            .iter()
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                Song::bytes_to_song(&value)
            })
            .collect()
    }

    fn find_by_key_contains(&self, search_term: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        let st = search_term.to_lowercase();
        self.songs_db
            .iter()
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                Some((key.to_vec(), value.to_vec()))
            })
            .filter(|(key, _)| String::from_utf8(key.clone()).is_ok_and(|k| k.to_lowercase().contains(&st)))
            .collect()
    }

    fn find_by_key_prefix(&self, prefix: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.songs_db
            .prefix(prefix.as_bytes())
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                Some((key.to_vec(), value.to_vec()))
            })
            .collect()
    }

    /// Returns all songs whose key (file path) starts with `prefix`.
    fn find_songs_by_dir_prefix(&self, prefix: &str) -> Vec<Song> {
        self.songs_db
            .prefix(prefix.as_bytes())
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                Song::bytes_to_song(&value)
            })
            .collect()
    }

    fn flush(&self) {
        // fjall handles persistence at the Database level; this is a no-op.
    }
}

#[cfg(test)]
mod test {
    use api_models::player::Song;

    use crate::{ports::song_repository::SongRepository, song_repository::FjallSongRepository, test::test_shared};

    macro_rules! insert_songs {
        ($repo:expr, $($file:expr, $title:expr, $artist:expr, $album:expr),* $(,)?) => {
            let db = &$repo.songs_db;
            $(
                db.insert($file, create_song($title, $artist, $album, $file))
                    .expect("Failed to insert song");
            )*
        };
    }
    fn create_song(title: &str, artist: &str, album: &str, file: &str) -> Vec<u8> {
        let song = Song {
            title: Some(title.to_owned()),
            artist: Some(artist.to_owned()),
            album: Some(album.to_owned()),
            file: file.to_owned(),
            ..Default::default()
        };
        song.to_json_string_bytes()
    }

    fn create_song_repo() -> FjallSongRepository {
        let ctx = test_shared::Context::default();
        FjallSongRepository::new_standalone(&ctx.db_dir)
    }

    #[test]
    fn should_get_songs() {
        let song_repository = create_song_repo();
        #[rustfmt::skip]
        insert_songs!(
            &song_repository,
            "hq/artist1/album1/file1", "title1", "artist1", "album1",
            "hq/artist1/album1/file2", "title2", "artist1", "album1",
            "hq test/comp1/file2", "title2", "artist2", "album2",
            "hq test/comp2/file3", "title3", "artist3", "album3",
        );
        let songs = song_repository.find_all();
        assert_eq!(songs.len(), 4);
    }
}
