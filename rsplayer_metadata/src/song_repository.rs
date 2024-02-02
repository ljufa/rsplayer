use sled::{Db, IVec};

use api_models::player::Song;

pub struct SongRepository {
    songs_db: Db,
}

impl SongRepository {
    pub fn new(db_path: &str) -> Self {
        Self {
            songs_db: sled::open(db_path).expect("Failed to open song db"),
        }
    }

    pub fn save(&self, song: &Song) {
        self.songs_db
            .insert(&song.file, song.to_json_string_bytes())
            .expect("Failed to save song");
    }
    pub fn delete(&self, id: &str) {
        self.songs_db.remove(id).expect("Failed to delete song");
    }
    pub fn delete_all(&self) {
        self.songs_db.clear().expect("Failed to delete all songs");
        self.songs_db.flush().expect("Failed to flush db");
    }

    pub fn find_by_id(&self, id: &str) -> Option<Song> {
        self.songs_db
            .get(id)
            .expect("Failed to get song")
            .map(|v| Song::bytes_to_song(v.as_ref()).expect("Failed to convert bytes to song"))
    }
    pub fn find_all(&self) -> Vec<Song> {
        self.songs_db
            .iter()
            .map(|v| {
                Song::bytes_to_song(v.expect("Failed to get song").1.as_ref()).expect("Failed to convert bytes to song")
            })
            .collect()
    }
    pub fn get_all_iterator(&self) -> impl Iterator<Item = Song> {
        self.songs_db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| Song::bytes_to_song(&s.1))
    }

    pub fn find_by_key_contains(&self, search_term: &str) -> impl Iterator<Item = (IVec, IVec)> {
        let st = search_term.to_lowercase();
        self.songs_db.iter().filter_map(Result::ok).filter(move |(key, _)| {
            let key_s = String::from_utf8(key.to_vec()).unwrap();
            key_s.to_lowercase().contains(&st)
        })
    }

    pub fn find_by_key_prefix(&self, prefix: &str) -> impl Iterator<Item = (IVec, IVec)> {
        self.songs_db.scan_prefix(prefix.as_bytes()).filter_map(Result::ok)
    }
    pub fn flush(&self) {
        self.songs_db.flush().expect("Failed to flush db");
    }
}

impl Default for SongRepository {
    fn default() -> Self {
        Self::new("songs.db")
    }
}

#[cfg(test)]
mod test {
    use api_models::player::Song;

    use crate::{song_repository::SongRepository, test::test_shared};

    macro_rules! insert_songs {
        ($repo:expr, $($file:expr, $title:expr, $artist:expr, $album:expr),* $(,)?) => {
            let db = &$repo.songs_db;
            $(
                db.insert($file, create_song($title, $artist, $album, $file))
                    .expect("Failed to insert song");
            )*
            db.flush().expect("Failed to flush DB");
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

    fn create_song_repo() -> SongRepository {
        let ctx = test_shared::Context::default();
        SongRepository::new(&ctx.db_dir)
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
