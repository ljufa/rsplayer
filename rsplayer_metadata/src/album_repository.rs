use chrono::DateTime;
use fjall::{Database, Keyspace, KeyspaceCreateOptions};

use api_models::{player::Song, playlist::Album};

use crate::genre_utils::{is_junk_genre, normalize_genre_key, normalize_name, resolve_id3v1_genre, title_case_genre};

pub struct AlbumRepository {
    albums_db: Keyspace,
}
impl AlbumRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            albums_db: db
                .keyspace("albums", KeyspaceCreateOptions::default)
                .expect("Failed to open albums keyspace"),
        }
    }

    pub fn delete_all(&self) {
        _ = self.albums_db.clear();
    }

    pub fn find_all_album_artists(&self) -> Vec<String> {
        let mut pairs: Vec<(String, String)> = self
            .find_all()
            .into_iter()
            .filter_map(|a| {
                let display = a.artist?;
                let key = normalize_name(&display);
                Some((key, display))
            })
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs.dedup_by(|a, b| a.0 == b.0);
        pairs.into_iter().map(|(_, display)| display).collect()
    }
    pub fn find_all(&self) -> Vec<Album> {
        self.albums_db
            .iter()
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                let mut album = Album::from_bytes(&value)?;
                album.id = String::from_utf8(key.to_vec()).ok()?;
                album.song_keys.clear();
                Some(album)
            })
            .collect()
    }
    pub fn find_by_id(&self, album_id: &str) -> Option<Album> {
        let normalized_key = normalize_name(album_id);
        if normalized_key.is_empty() {
            return None;
        }
        let bytes = self
            .albums_db
            .get(normalized_key.as_bytes())
            .expect("Album DB error")
            .or_else(|| self.albums_db.get(album_id.as_bytes()).expect("Album DB error"))?;

        let mut album = Album::from_bytes(&bytes)?;
        album_id.clone_into(&mut album.id);
        Some(album)
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
    pub fn find_all_by_genre(&self, limit_per_genre: usize) -> Vec<(String, Vec<Album>)> {
        let albums = self.find_all();
        let mut genre_map: std::collections::HashMap<String, (String, Vec<Album>)> = std::collections::HashMap::new();
        for album in albums {
            if let Some(ref raw_genre) = album.genre {
                let genre_str = resolve_id3v1_genre(raw_genre).map_or_else(|| raw_genre.clone(), String::from);
                if genre_str.is_empty() {
                    continue;
                }
                let key = normalize_genre_key(&genre_str);
                if is_junk_genre(&key) {
                    continue;
                }
                let entry = genre_map
                    .entry(key)
                    .or_insert_with(|| (title_case_genre(&genre_str), Vec::new()));
                entry.1.push(album);
            }
        }
        let mut result: Vec<(String, Vec<Album>)> = genre_map
            .into_iter()
            .filter(|(_, (_, albums))| albums.len() >= 2)
            .map(|(_, (display_name, mut albums))| {
                albums.sort_by(|a, b| b.added.cmp(&a.added));
                albums.truncate(limit_per_genre);
                (display_name, albums)
            })
            .collect();
        result.sort_by_key(|a| a.0.to_lowercase());
        result
    }

    pub fn find_all_by_decade(&self, limit_per_decade: usize) -> Vec<(String, Vec<Album>)> {
        let albums = self.find_all();
        let mut decade_map: std::collections::HashMap<String, Vec<Album>> = std::collections::HashMap::new();
        for album in albums {
            if let Some(released) = album.released {
                let year_str = released.format("%Y").to_string();
                if let Ok(year) = year_str.parse::<u32>() {
                    if year >= 1950 {
                        let decade = format!("{}0s", &year_str[..3]);
                        decade_map.entry(decade).or_default().push(album);
                    }
                }
            }
        }
        let mut result: Vec<(String, Vec<Album>)> = decade_map
            .into_iter()
            .filter(|(_, albums)| albums.len() >= 2)
            .map(|(decade, mut albums)| {
                albums.sort_by(|a, b| b.released.cmp(&a.released));
                albums.truncate(limit_per_decade);
                (decade, albums)
            })
            .collect();
        result.sort_by(|a, b| b.0.cmp(&a.0));
        result
    }

    pub fn find_by_artist(&self, artist: &str) -> Vec<Album> {
        let normalized_query = normalize_name(artist);
        self.albums_db
            .iter()
            .filter_map(|guard| {
                let (key, value) = guard.into_inner().ok()?;
                let mut album = Album::from_bytes(&value)?;
                album.id = String::from_utf8(key.to_vec()).ok()?;
                Some(album)
            })
            .filter(|a| a.artist.as_ref().is_some_and(|a| normalize_name(a) == normalized_query))
            .collect()
    }

    pub fn album_db_key(artist: &str, album: &str) -> String {
        let na = normalize_name(artist);
        let nb = normalize_name(album);
        if na.is_empty() {
            nb
        } else {
            format!("{na}|{nb}")
        }
    }

    pub fn update_from_song(&self, song: Song) -> anyhow::Result<()> {
        let raw_album = match song.album.as_ref() {
            Some(a) if !a.is_empty() => a.clone(),
            _ => return Ok(()),
        };
        let artist_for_key = song
            .album_artist
            .as_deref()
            .or(song.artist.as_deref())
            .unwrap_or("");
        let key = Self::album_db_key(artist_for_key, &raw_album);
        if key.is_empty() {
            return Ok(());
        }
        let existing_album = self
            .albums_db
            .get(key.as_bytes())
            .map_err(|e| anyhow::anyhow!("Album DB read error for '{raw_album}': {e}"))?;
        let mut album = existing_album
            .and_then(|bytes| Album::from_bytes(&bytes))
            .unwrap_or_default();

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
        if album.title.is_empty() {
            album.title = raw_album;
        }
        album.added = song.file_date;
        self.albums_db
            .insert(key.as_bytes(), album.to_json_string_bytes())
            .map_err(|e| anyhow::anyhow!("Album DB write error for '{}': {e}", album.title))
    }
}

impl AlbumRepository {
    pub fn new_standalone(db_path: &str) -> Self {
        let db = Database::builder(db_path).open().expect("Failed to open albums db");
        Self {
            albums_db: db
                .keyspace("albums", KeyspaceCreateOptions::default)
                .expect("Failed to open albums keyspace"),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::album_repository::AlbumRepository;
    use crate::test::test_shared;
    use api_models::playlist::Album;
    use chrono::{Months, Utc};

    macro_rules! insert_albums_with_date {
        ($repo:expr, $($key:expr, $title:expr, $artist:expr, $added_offset:expr, $published_offset:expr),* $(,)?) => {
            let db = &$repo.albums_db;
            $( db.insert($key, create_album($title, $artist, None, $added_offset, $published_offset)).expect("Failed to insert album"); )*
        };
    }
    macro_rules! insert_albums {
        ($repo:expr, $($key:expr, $title:expr, $artist:expr, $genre:expr),* $(,)?) => {
            let db = &$repo.albums_db;
            $( db.insert($key, create_album($title, $artist, $genre, None, None)).expect("Failed to insert album"); )*
        };
    }

    #[test]
    fn should_get_albums() {
        let album_repository = create_album_repo();
        insert_albums!(
            &album_repository,
            "a1",
            "Album One",
            "RP and E Goldstein",
            Some("Classical"),
            "a2",
            "Album Two",
            "Artist 1",
            Some("Club")
        );
        let albums = album_repository.find_all();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].title, "Album One");
        assert_eq!(albums[0].artist, Some("RP and E Goldstein".to_owned()));
        assert_eq!(albums[1].title, "Album Two");
    }

    #[test]
    fn should_get_latest_added_albums() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums_with_date!(&album_repository,
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
        insert_albums_with_date!(&album_repository,
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
        insert_albums!(&album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club"),
            "a3", "Album Three", "RP and E Goldstein", Some("Classical"),
            "a4", "Album Four", "Artist 2", Some("Club"),
            "a5", "Album Five", "RP and E Goldstein", Some("Classical"),
            "a6", "Album Six", "Artist 3", Some("Club"),
        );
        let result = album_repository.find_all_album_artists();
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_find_by_artist() {
        let album_repository = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(&album_repository,
            "a1", "Album One", "RP and E Goldstein", Some("Classical"),
            "a2", "Album Two", "Artist 1", Some("Club"),
            "a3", "Album Three", "RP and E Goldstein", Some("Classical"),
            "a4", "Album Four", "Artist 2", Some("Club"),
            "a5", "Album Five", "RP and E Goldstein", Some("Classical"),
            "a6", "Album Six", "Artist 3", Some("Club"),
        );
        let mut result = album_repository.find_by_artist("RP and E Goldstein");
        assert_eq!(result.len(), 3);
        result = album_repository.find_by_artist("Artist 1");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn normalize_name_case() {
        use super::normalize_name;
        assert_eq!(normalize_name("Pink Floyd"), normalize_name("pink floyd"));
    }
    #[test]
    fn normalize_name_whitespace() {
        use super::normalize_name;
        assert_eq!(normalize_name("  Pink  Floyd  "), normalize_name("Pink Floyd"));
    }
    #[test]
    fn normalize_name_diacritics() {
        use super::normalize_name;
        assert_eq!(normalize_name("Beyoncé"), normalize_name("Beyonce"));
    }
    #[test]
    fn normalize_name_punctuation() {
        use super::normalize_name;
        assert_eq!(normalize_name("AC\u{2013}DC"), normalize_name("AC-DC"));
    }

    #[test]
    fn update_from_song_merges_case_variants() {
        use api_models::player::Song;
        use chrono::Utc;
        let repo = create_album_repo();
        for (i, album_name) in [
            "Dark Side of the Moon",
            "dark side of the moon",
            "Dark Side Of The Moon",
        ]
        .iter()
        .enumerate()
        {
            repo.update_from_song(Song {
                file: format!("artist/album/track{i}.flac"),
                album: Some(album_name.to_string()),
                artist: Some("Pink Floyd".to_string()),
                file_date: Utc::now(),
                ..Default::default()
            })
            .expect("update_from_song failed");
        }
        let all = repo.find_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Dark Side of the Moon");
        let key = AlbumRepository::album_db_key("Pink Floyd", "Dark Side of the Moon");
        let full = repo.find_by_id(&key).expect("album not found");
        assert_eq!(full.song_keys.len(), 3);
    }

    #[test]
    fn find_all_album_artists_deduplicates_case_variants() {
        use api_models::player::Song;
        use chrono::Utc;
        let repo = create_album_repo();
        for (i, artist) in ["Pink Floyd", "pink floyd", "PINK FLOYD"].iter().enumerate() {
            repo.update_from_song(Song {
                file: format!("track{i}.flac"),
                album: Some(format!("Album {i}")),
                artist: Some(artist.to_string()),
                file_date: Utc::now(),
                ..Default::default()
            })
            .expect("update_from_song failed");
        }
        let artists = repo.find_all_album_artists();
        assert_eq!(artists.len(), 1);
    }

    #[test]
    fn query_songs_by_album_roundtrip() {
        use api_models::player::Song;
        use chrono::Utc;
        let repo = create_album_repo();
        repo.update_from_song(Song {
            file: "pink_floyd/dsotm/money.flac".to_string(),
            album: Some("Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        })
        .expect("update_from_song failed");
        repo.update_from_song(Song {
            file: "pink_floyd/dsotm/time.flac".to_string(),
            album: Some("Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        })
        .expect("update_from_song failed");
        let albums = repo.find_by_artist("Pink Floyd");
        assert_eq!(albums.len(), 1);
        let found = repo.find_by_id(&albums[0].id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().song_keys.len(), 2);
    }

    #[test]
    fn find_by_artist_is_case_insensitive() {
        use api_models::player::Song;
        use chrono::Utc;
        let repo = create_album_repo();
        repo.update_from_song(Song {
            file: "track1.flac".to_string(),
            album: Some("Wish You Were Here".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        })
        .expect("update_from_song failed");
        assert_eq!(repo.find_by_artist("pink floyd").len(), 1);
        assert_eq!(repo.find_by_artist("PINK FLOYD").len(), 1);
    }

    #[test]
    fn test_delete_all() {
        // Keep ctx alive so the database directory exists on disk — clear() needs it.
        let ctx = test_shared::Context::default();
        let album_repository = AlbumRepository::new_standalone(&ctx.db_dir);
        insert_albums!(
            &album_repository,
            "a1",
            "Album One",
            "Artist",
            Some("Classical"),
            "a2",
            "Album Two",
            "Artist",
            Some("Club")
        );
        album_repository.delete_all();
        assert_eq!(album_repository.find_all().len(), 0);
    }

    #[test]
    fn resolve_id3v1_numeric_genres() {
        use super::resolve_id3v1_genre;
        assert_eq!(resolve_id3v1_genre("(17)"), Some("Rock"));
        assert_eq!(resolve_id3v1_genre("17"), Some("Rock"));
        assert_eq!(resolve_id3v1_genre("(999)"), None);
    }
    #[test]
    fn junk_genres_are_filtered() {
        use super::is_junk_genre;
        assert!(is_junk_genre("other"));
        assert!(!is_junk_genre("rock"));
    }
    #[test]
    fn genre_title_case() {
        use super::title_case_genre;
        assert_eq!(title_case_genre("progressive rock"), "Progressive Rock");
    }

    #[test]
    fn find_all_by_genre_merges_case_variants() {
        let repo = create_album_repo();
        insert_albums!(
            &repo,
            "a1",
            "Album One",
            "Artist 1",
            Some("Electronic"),
            "a2",
            "Album Two",
            "Artist 2",
            Some("electronic"),
            "a3",
            "Album Three",
            "Artist 3",
            Some("ELECTRONIC"),
            "a4",
            "Album Four",
            "Artist 4",
            Some("Rock"),
            "a5",
            "Album Five",
            "Artist 5",
            Some("rock")
        );
        let result = repo.find_all_by_genre(20);
        let electronic = result.iter().find(|(name, _)| name.to_lowercase() == "electronic");
        assert!(electronic.is_some());
        assert_eq!(electronic.unwrap().1.len(), 3);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn find_all_by_genre_resolves_id3v1_codes() {
        let repo = create_album_repo();
        insert_albums!(
            &repo,
            "a1",
            "Album One",
            "Artist 1",
            Some("(17)"),
            "a2",
            "Album Two",
            "Artist 2",
            Some("Rock")
        );
        let result = repo.find_all_by_genre(20);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.len(), 2);
    }

    #[test]
    fn find_all_by_genre_filters_junk() {
        let repo = create_album_repo();
        insert_albums!(
            &repo,
            "a1",
            "A1",
            "Ar1",
            Some("Other"),
            "a2",
            "A2",
            "Ar2",
            Some("Other"),
            "a3",
            "A3",
            "Ar3",
            Some("Unknown genre"),
            "a4",
            "A4",
            "Ar4",
            Some("Unknown genre"),
            "a5",
            "A5",
            "Ar5",
            Some("Jazz"),
            "a6",
            "A6",
            "Ar6",
            Some("Jazz")
        );
        let result = repo.find_all_by_genre(20);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "Jazz");
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
        AlbumRepository::new_standalone(&ctx.db_dir)
    }
}
