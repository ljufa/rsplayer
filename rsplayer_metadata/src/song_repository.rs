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
        let result = self
            .get_all_iterator()
            .filter(|song| song.file.starts_with(dir))
            .map(|song| {
                let Some((_, right)) = song.file.split_once(dir) else {
                    return MetadataLibraryItem::Empty;
                };
                if right.contains('/') {
                    let Some((left, _)) = right.split_once('/') else {
                        return MetadataLibraryItem::Empty;
                    };
                    return MetadataLibraryItem::Directory { name: left.to_owned() };
                }
                MetadataLibraryItem::SongItem(song)
            });
        let mut result_vec: Vec<MetadataLibraryItem> = result.collect();
        result_vec.dedup();
        MetadataLibraryResult {
            items: result_vec,
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
