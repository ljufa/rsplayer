use chrono::DateTime;
use sled::Db;

use api_models::{player::Song, playlist::Album};

pub struct AlbumRepository {
    albums_db: Db,
}
impl AlbumRepository {
    pub fn new(db_path: &str) -> Self {
        let db = sled::open(db_path).expect("Failed to open albums db");
        Self { albums_db: db }
    }

    pub fn delete_all(&self) {
        self.albums_db.clear().expect("Failed to clear albums db");
    }

    pub fn find_all_album_artists(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .find_all()
            .iter()
            .map(|a| a.artist.clone().unwrap_or_default())
            .collect();
        result.sort();
        result.dedup();
        result
    }
    pub fn find_all(&self) -> Vec<Album> {
        self.albums_db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| {
                let mut album = Album::from_bytes(&s.1);
                album.id = String::from_utf8(s.0.to_vec()).unwrap();
                album.song_keys.clear();
                Some(album)
            })
            .collect()
    }
    pub fn find_by_id(&self, album_id: &str) -> Option<Album> {
        self.albums_db
            .get(album_id.as_bytes())
            .expect("Album DB error")
            .map(|bytes| {
                let mut album = Album::from_bytes(&bytes);
                album_id.clone_into(&mut album.id);
                album
            })
    }

    pub fn find_all_sort_by_added_desc(&self, limit: usize) -> Vec<Album> {
        let mut albums = self.find_all();
        albums.sort_by(|a, b| b.added.cmp(&a.added));
        albums.truncate(limit);
        albums
    }
    pub fn find_all_sort_by_released_desc(&self, limit: usize) -> Vec<Album> {
        let mut albums = self.find_all();
        albums.sort_by(|a, b| b.released.cmp(&a.released));
        albums.truncate(limit);
        albums
    }
    pub fn find_by_artist(&self, artist: &str) -> Vec<Album> {
        self.albums_db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| Some(Album::from_bytes(&s.1)))
            .filter(|a| a.artist.as_ref().is_some_and(|a| a == artist))
            .collect()
    }

    pub fn update_from_song(&self, song: Song) {
        let key = song.album.as_ref().map_or(String::new(), std::clone::Clone::clone);
        if key.is_empty() {
            return;
        }
        let existing_album = self.albums_db.get(&key).expect("Album DB error");
        let mut album = existing_album.map_or_else(Album::default, |bytes| Album::from_bytes(&bytes));
        if !album.song_keys.contains(&song.file) {
            album.song_keys.push(song.file);
        }
        if song.image_id.is_some() {
            album.image_id = song.image_id;
        }

        if let Some(artist) = song.album_artist {
            album.artist = Some(artist);
        } else if let Some(artist) = song.artist {
            album.artist = Some(artist);
        }
        if let Some(date) = song.date {
            if date.len() == 4 {
                if let Ok(dt) = DateTime::parse_from_rfc3339(&format!("{}-01-01T00:00:00Z", &date)) {
                    album.released = Some(dt.naive_utc().and_utc());
                }
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&date) {
                album.released = Some(dt.naive_utc().and_utc());
            }
        } else if let Some(year) = song.tags.get("year") {
            if let Ok(dt) = DateTime::parse_from_rfc3339(&format!("{year}-01-01T00:00:00Z")) {
                album.released = Some(dt.naive_utc().and_utc());
            }
        }
        if let Some(genre) = song.genre {
            album.genre = Some(genre);
        }
        if let Some(label) = song.label {
            album.label = Some(label);
        }
        if let Some(title) = song.album {
            album.title = title;
        }
        album.added = song.file_date;
        _ = self.albums_db.insert(&key, album.to_json_string_bytes());
        drop(album);
    }
}

impl Default for AlbumRepository {
    fn default() -> Self {
        Self::new("albums.db")
    }
}

#[cfg(test)]
mod test {
    use chrono::{Months, Utc};

    use api_models::playlist::Album;

    use crate::album_repository::AlbumRepository;
    use crate::test::test_shared;

    macro_rules! insert_albums_with_date {
        ($repo:expr, $($key:expr, $title:expr, $artist:expr, $added_offset:expr, $published_offset:expr),* $(,)?) => {
            let db = &$repo.albums_db;
            $(
                db.insert($key, create_album($title, $artist, None, $added_offset, $published_offset))
                    .expect("Failed to insert album");
            )*
            db.flush().expect("Failed to flush DB");
        };
    }
    macro_rules! insert_albums {
        ($repo:expr, $($key:expr, $title:expr, $artist:expr, $genre:expr),* $(,)?) => {
            let db = &$repo.albums_db;
            $(
                db.insert($key, create_album($title, $artist, $genre, None, None))
                    .expect("Failed to insert album");
            )*
            db.flush().expect("Failed to flush DB");
        };
    }

    #[test]
    fn should_get_albums() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club")
        );
        let albums = album_repository.find_all();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].title, "Album One");
        assert_eq!(albums[0].artist, Some("RP and E Goldstein".to_owned()));
        assert_eq!(albums[0].genre, Some("Classical".to_owned()));

        assert_eq!(albums[1].title, "Album Two");
        assert_eq!(albums[1].artist, Some("Artist 1".to_owned()));
        assert_eq!(albums[1].genre, Some("Club".to_owned()));
    }

    #[test]
    fn should_get_latest_added_albums() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums_with_date!(
            &album_repository, 
            "a4", "Album 7", "Artist 2", Some(-7), None,
            "a4", "Album 6", "Artist 2", Some(-6), None,
            "a1", "Album 1", "Artist 1", Some(-1), None,
            "a4", "Album 5", "Artist 2", Some(-5), None,
            "a2", "Album 2", "Artist 1", Some(-2), None,
            "a3", "Album 3", "Artist 2", Some(-3), None,
            "a4", "Album 4", "Artist 2", Some(-4), None,
        );
        let result = album_repository.find_all_sort_by_added_desc(3);
        assert_eq!(result.len(), 3);
        assert!(result[0].title.contains("Album 1"));
        assert!(result[1].title.contains("Album 2"));
        assert!(result[2].title.contains("Album 3"));
    }

    #[test]
    fn should_get_latest_released_albums() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums_with_date!(
            &album_repository,
            "a7", "Album 7", "Artist 2", None, Some(-4),
            "a6", "Album 6", "Artist 2", None, Some(-2),
            "a1", "Album 1", "Artist 1", None, Some(-6),
            "a5", "Album 5", "Artist 2", None, Some(-1),
            "a2", "Album 2", "Artist 1", None, Some(-1),
            "a3", "Album 3", "Artist 2", None, Some(-3),
            "a4", "Album 4", "Artist 2", None, Some(-6),
        );

        let result = album_repository.find_all_sort_by_released_desc(3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].title, "Album 2");
        assert_eq!(result[1].title, "Album 5");
        assert_eq!(result[2].title, "Album 6");
    }

    #[test]
    fn test_find_all_album_artists() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club"),
            "a3", "Album Three", "RP and E Goldstein", Some("Classical"),
            "a4", "Album Four", "Artist 2", Some("Club"),
            "a5", "Album Five", "RP and E Goldstein", Some("Classical"),
            "a6", "Album Six", "Artist 3", Some("Club"),
        );
        let result = album_repository.find_all_album_artists();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], "Artist 1");
        assert_eq!(result[1], "Artist 2");
        assert_eq!(result[2], "Artist 3");
        assert_eq!(result[3], "RP and E Goldstein");
    }

    #[test]
    fn test_find_by_artist() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club"),
            "a3", "Album Three", "RP and E Goldstein", Some("Classical"),
            "a4", "Album Four", "Artist 2", Some("Club"),
            "a5", "Album Five", "RP and E Goldstein", Some("Classical"),
            "a6", "Album Six", "Artist 3", Some("Club"),
        );
        let mut result = album_repository.find_by_artist("RP and E Goldstein");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].title, "Album One");
        assert_eq!(result[1].title, "Album Three");
        assert_eq!(result[2].title, "Album Five");

        result = album_repository.find_by_artist("Artist 1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Album Two");
    }

    #[test]
    fn test_delete_all() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club"),
            "a3", "Album Three", "RP and E Goldstein", Some("Classical"),
            "a4", "Album Four", "Artist 2", Some("Club"),
            "a5", "Album Five", "RP and E Goldstein", Some("Classical"),
            "a6", "Album Six", "Artist 3", Some("Club"),
        );
        album_repository.delete_all();
        let result = album_repository.find_all();
        assert_eq!(result.len(), 0);
    }

    fn create_album(
        title: &str,
        artist: &str,
        genre: Option<&str>,
        added: Option<i32>,
        published: Option<i32>,
    ) -> Vec<u8> {
        let added_date = added.map_or_else(Utc::now, |add| {
            if add < 0 {
                chrono::Utc::now()
                    .checked_sub_months(Months::new(add.unsigned_abs()))
                    .unwrap()
            } else {
                chrono::Utc::now()
                    .checked_add_months(Months::new(add.unsigned_abs()))
                    .unwrap()
            }
        });
        let published_date = published.map(|add| {
            if add < 0 {
                chrono::Utc::now()
                    .checked_sub_months(Months::new(add.unsigned_abs()))
                    .unwrap()
            } else {
                chrono::Utc::now()
                    .checked_add_months(Months::new(add.unsigned_abs()))
                    .unwrap()
            }
        });
        Album {
            title: title.to_owned(),
            artist: Some(artist.to_owned()),
            added: added_date,
            released: published_date,
            genre: genre.map(std::borrow::ToOwned::to_owned),
            ..Default::default()
        }
        .to_json_string_bytes()
    }
    fn create_album_repo() -> AlbumRepository {
        let ctx = test_shared::Context::default();
        AlbumRepository::new(&ctx.db_dir)
    }
}
