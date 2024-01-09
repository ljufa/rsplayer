use api_models::common::{MetadataLibraryItem, MetadataLibraryResult};
use sled::Db;

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
    pub fn find_by_dir(&self, dir: &str) -> MetadataLibraryResult {
        let start_time = std::time::Instant::now();

        let result = self
            .songs_db
            .scan_prefix(dir.as_bytes())
            .filter_map(std::result::Result::ok)
            .map(|(key, value)| {
                let key = String::from_utf8(key.to_vec()).unwrap();
                let Some((_, right)) = key.split_once(dir) else {
                    return MetadataLibraryItem::Empty;
                };
                if right.contains('/') {
                    let Some((left, _)) = right.split_once('/') else {
                        return MetadataLibraryItem::Empty;
                    };
                    MetadataLibraryItem::Directory { name: left.to_owned() }
                } else {
                    MetadataLibraryItem::SongItem(Song::bytes_to_song(&value).expect(
                        "Failed to
                         convert bytes to song",
                    ))
                }
            });
        let mut unique: Vec<MetadataLibraryItem> = result.collect();
        unique.dedup();
        log::info!("find_by_dir took {:?}", start_time.elapsed());
        MetadataLibraryResult {
            items: unique,
            root_path: dir.to_owned(),
        }
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
    use api_models::{common::MetadataLibraryItem, player::Song};

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

    #[test]
    fn test_find_by_dir() {
        let song_repository = create_song_repo();
        #[rustfmt::skip]
        insert_songs!(
            &song_repository,
            "hq/artist1/album1/file1", "title1", "artist1", "album1",
            "hq/artist1/album1/file2", "title2", "artist1", "album1",
            "hq/artist2/album2/file1", "title4", "artist4", "album4",
            "hq/file1", "title5", "artist5", "album4",
            "hq test/comp1/file2", "title2", "artist2", "album2",
            "hq test/comp2/file3", "title3", "artist3", "album3",
        );
        let result = song_repository.find_by_dir("hq/");
        assert_eq!(result.root_path, "hq/");
        assert_eq!(result.items.len(), 3);
        assert_eq!(
            result.items[0],
            MetadataLibraryItem::Directory {
                name: "artist1".to_owned()
            }
        );
        assert_eq!(
            result.items[1],
            MetadataLibraryItem::Directory {
                name: "artist2".to_owned()
            }
        );
        assert_eq!(result.items[2].get_title(), "title5");

        let result = song_repository.find_by_dir("hq test/");
        assert_eq!(result.items.len(), 2);
        assert_eq!(
            result.items[0],
            MetadataLibraryItem::Directory {
                name: "comp1".to_owned()
            }
        );
    }
}
