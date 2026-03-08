use chrono::DateTime;
use sled::Db;
use unicode_normalization::UnicodeNormalization;

use api_models::{player::Song, playlist::Album};

/// Produce a canonical grouping key from an artist or album name.
///
/// The transformations applied (in order):
/// 1. NFD decompose → drop combining (diacritic) code-points → recompose as NFC
///    e.g. "Beyoncé" → "Beyonce"
/// 2. Lowercase
/// 3. Collapse runs of whitespace to a single space and trim edges
/// 4. Normalise common punctuation variants:
///    - en-dash / em-dash / hyphen variants → ASCII hyphen '-'
///    - smart/curly quotes → straight ASCII quote
///    - forward-slash variants → '/'
///
/// The original display string is kept unchanged in the `Album` struct; only
/// the *key* used for sled lookups and dedup is normalised.
pub(crate) fn normalize_name(s: &str) -> String {
    // Step 1: strip diacritics via NFD + filter combining chars + NFC
    let without_diacritics: String = s
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .nfc()
        .collect();

    // Step 2: lowercase
    let lower = without_diacritics.to_lowercase();

    // Step 3: collapse internal whitespace, trim
    let collapsed = lower.split_whitespace().collect::<Vec<_>>().join(" ");

    // Step 4: normalise punctuation
    collapsed
        .chars()
        .map(|c| match c {
            // dash variants → ASCII hyphen
            '\u{2013}' | '\u{2014}' | '\u{2012}' | '\u{2015}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => '-',
            // smart/curly single quotes → apostrophe
            '\u{2018}' | '\u{2019}' | '\u{02BC}' => '\'',
            // smart/curly double quotes → straight double quote
            '\u{201C}' | '\u{201D}' => '"',
            // full-width slash → ASCII slash
            '\u{FF0F}' => '/',
            other => other,
        })
        .collect()
}

/// ID3v1 genre lookup table. Index = numeric genre code, value = genre name.
/// See <https://en.wikipedia.org/wiki/List_of_ID3v1_genres>
const ID3V1_GENRES: &[&str] = &[
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    "Alternative Rock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta",
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native US",
    "Cabaret",
    "New Wave",
    "Psychedelic",
    "Rave",
    "Showtunes",
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
    // Extended (80+)
    "Folk",
    "Folk-Rock",
    "National Folk",
    "Swing",
    "Fast Fusion",
    "Bebop",
    "Latin",
    "Revival",
    "Celtic",
    "Bluegrass",
    "Avantgarde",
    "Gothic Rock",
    "Progressive Rock",
    "Psychedelic Rock",
    "Symphonic Rock",
    "Slow Rock",
    "Big Band",
    "Chorus",
    "Easy Listening",
    "Acoustic",
    "Humour",
    "Speech",
    "Chanson",
    "Opera",
    "Chamber Music",
    "Sonata",
    "Symphony",
    "Booty Bass",
    "Primus",
    "Porn Groove",
    "Satire",
    "Slow Jam",
    "Club",
    "Tango",
    "Samba",
    "Folklore",
    "Ballad",
    "Power Ballad",
    "Rhythmic Soul",
    "Freestyle",
    "Duet",
    "Punk Rock",
    "Drum Solo",
    "A Capella",
    "Euro-House",
    "Dance Hall",
    "Goa",
    "Drum & Bass",
    "Club-House",
    "Hardcore Techno",
    "Terror",
    "Indie",
    "BritPop",
    "Negerpunk",
    "Polsk Punk",
    "Beat",
    "Christian Gangsta Rap",
    "Heavy Metal",
    "Black Metal",
    "Crossover",
    "Contemporary Christian",
    "Christian Rock",
    "Merengue",
    "Salsa",
    "Thrash Metal",
    "Anime",
    "JPop",
    "Synthpop",
    "Abstract",
    "Art Rock",
    "Baroque",
    "Bhangra",
    "Big Beat",
    "Breakbeat",
    "Chillout",
    "Downtempo",
    "Dub",
    "EBM",
    "Eclectic",
    "Electro",
    "Electroclash",
    "Emo",
    "Experimental",
    "Garage",
    "Global",
    "IDM",
    "Illbient",
    "Industro-Goth",
    "Jam Band",
    "Krautrock",
    "Leftfield",
    "Lounge",
    "Math Rock",
    "New Romantic",
    "Nu-Breakz",
    "Post-Punk",
    "Post-Rock",
    "Psytrance",
    "Shoegaze",
    "Space Rock",
    "Trop Rock",
    "World Music",
    "Neoclassical",
    "Audiobook",
    "Audio Theatre",
    "Neue Deutsche Welle",
    "Podcast",
    "Indie-Rock",
    "G-Funk",
    "Dubstep",
    "Garage Rock",
    "Psybient",
];

/// Resolve a raw genre string that might be an ID3v1 numeric code like "(17)" or "17".
fn resolve_id3v1_genre(raw: &str) -> Option<&'static str> {
    let trimmed = raw.trim();
    // Match "(N)" or just "N"
    let num_str = trimmed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed);
    let idx: usize = num_str.parse().ok()?;
    ID3V1_GENRES.get(idx).copied()
}

/// Genres to exclude from the category listing.
fn is_junk_genre(normalized: &str) -> bool {
    matches!(
        normalized,
        "other" | "unknown genre" | "unknown" | "misc" | "none" | "unclassified" | ""
    )
}

/// Produce a normalized lowercase key for grouping genres.
///
/// - Strips diacritics (reuses the existing `normalize_name` pipeline)
/// - Collapses whitespace
/// - Normalises separators: " / ", "/", " - ", etc. to a single canonical form
fn normalize_genre_key(genre: &str) -> String {
    normalize_name(genre)
}

/// Title-case a genre string for display: "progressive rock" → "Progressive Rock".
fn title_case_genre(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

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
        // Collect (normalized_key, original_display) pairs, then dedup on the key
        // so that e.g. "Pink Floyd" and "pink floyd" collapse to one entry while
        // the display string returned is the first one encountered (after sorting).
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
        // Try the normalized key first (new-style records written after the normalization fix).
        // Fall back to the verbatim key for backwards compatibility with databases that were
        // scanned before the fix and still use the raw album-title string as the sled key.
        let normalized_key = normalize_name(album_id);
        let bytes = self
            .albums_db
            .get(normalized_key.as_bytes())
            .expect("Album DB error")
            .or_else(|| self.albums_db.get(album_id.as_bytes()).expect("Album DB error"))?;

        let mut album = Album::from_bytes(&bytes);
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
        // Maps: normalized_key → (display_name, albums)
        let mut genre_map: std::collections::HashMap<String, (String, Vec<Album>)> = std::collections::HashMap::new();
        for album in albums {
            if let Some(ref raw_genre) = album.genre {
                // Resolve ID3v1 numeric codes like "(17)" → "Rock"
                let genre_str = resolve_id3v1_genre(raw_genre)
                    .map(String::from)
                    .unwrap_or_else(|| raw_genre.clone());

                if genre_str.is_empty() {
                    continue;
                }

                let key = normalize_genre_key(&genre_str);
                if is_junk_genre(&key) {
                    continue;
                }

                let entry = genre_map.entry(key).or_insert_with(|| {
                    // Use title-cased version of the first occurrence as display name
                    (title_case_genre(&genre_str), Vec::new())
                });
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
        result.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
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
            .filter_map(std::result::Result::ok)
            .map_while(|s| Some(Album::from_bytes(&s.1)))
            .filter(|a| a.artist.as_ref().is_some_and(|a| normalize_name(a) == normalized_query))
            .collect()
    }

    pub fn update_from_song(&self, song: Song) {
        let raw_album = match song.album.as_ref() {
            Some(a) if !a.is_empty() => a.clone(),
            _ => return,
        };
        // Use the normalized form as the DB key so that minor spelling/case/diacritic
        // variants of the same album title all land in the same record.
        let key = normalize_name(&raw_album);

        let existing_album = self.albums_db.get(key.as_bytes()).expect("Album DB error");
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
        // Keep the first (prettiest) display title we saw; don't let a lowercase
        // variant overwrite a properly-cased one that was stored earlier.
        if album.title.is_empty() {
            album.title = raw_album;
        }
        album.added = song.file_date;
        _ = self.albums_db.insert(key.as_bytes(), album.to_json_string_bytes());
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

    // --- Normalization tests ---

    #[test]
    fn normalize_name_case() {
        use super::normalize_name;
        assert_eq!(normalize_name("Pink Floyd"), normalize_name("pink floyd"));
        assert_eq!(normalize_name("PINK FLOYD"), normalize_name("Pink Floyd"));
    }

    #[test]
    fn normalize_name_whitespace() {
        use super::normalize_name;
        assert_eq!(normalize_name("  Pink  Floyd  "), normalize_name("Pink Floyd"));
        assert_eq!(normalize_name("Pink\tFloyd"), normalize_name("Pink Floyd"));
    }

    #[test]
    fn normalize_name_diacritics() {
        use super::normalize_name;
        assert_eq!(normalize_name("Beyoncé"), normalize_name("Beyonce"));
        assert_eq!(normalize_name("Björk"), normalize_name("Bjork"));
        assert_eq!(normalize_name("Sigur Rós"), normalize_name("Sigur Ros"));
    }

    #[test]
    fn normalize_name_punctuation() {
        use super::normalize_name;
        // en-dash / em-dash → hyphen
        assert_eq!(normalize_name("AC\u{2013}DC"), normalize_name("AC-DC"));
        assert_eq!(normalize_name("AC\u{2014}DC"), normalize_name("AC-DC"));
        // smart quotes → straight quotes
        assert_eq!(normalize_name("\u{2018}Heroes\u{2019}"), normalize_name("'Heroes'"));
        assert_eq!(normalize_name("\u{201C}Heroes\u{201D}"), normalize_name("\"Heroes\""));
    }

    #[test]
    fn update_from_song_merges_case_variants() {
        use api_models::player::Song;
        use chrono::Utc;

        let repo = create_album_repo();

        let song1 = Song {
            file: "artist/album/track1.flac".to_string(),
            album: Some("Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        };
        let song2 = Song {
            file: "artist/album/track2.flac".to_string(),
            album: Some("dark side of the moon".to_string()), // lowercase variant
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        };
        let song3 = Song {
            file: "artist/album/track3.flac".to_string(),
            album: Some("Dark Side Of The Moon".to_string()), // title-case variant
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        };

        repo.update_from_song(song1);
        repo.update_from_song(song2);
        repo.update_from_song(song3);

        let all = repo.find_all();
        // All three variants should have merged into a single album record
        assert_eq!(
            all.len(),
            1,
            "expected 1 album, got {}: {:?}",
            all.len(),
            all.iter().map(|a| &a.title).collect::<Vec<_>>()
        );
        // The display title should be the first one seen (properly cased)
        assert_eq!(all[0].title, "Dark Side of the Moon");
        // Use find_by_id to get the full record including song_keys (find_all clears them)
        let full = repo.find_by_id("Dark Side of the Moon").expect("album not found");
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
            });
        }

        let artists = repo.find_all_album_artists();
        assert_eq!(artists.len(), 1, "expected 1 unique artist, got {:?}", artists);
    }

    #[test]
    fn query_songs_by_album_roundtrip() {
        // Simulates the exact sequence: scan songs → QueryAlbumsByArtist → QuerySongsByAlbum
        use api_models::player::Song;
        use chrono::Utc;

        let repo = create_album_repo();
        repo.update_from_song(Song {
            file: "pink_floyd/dsotm/money.flac".to_string(),
            album: Some("Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        });
        repo.update_from_song(Song {
            file: "pink_floyd/dsotm/time.flac".to_string(),
            album: Some("Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            file_date: Utc::now(),
            ..Default::default()
        });

        // Step 1: QueryAlbumsByArtist — get the album name the UI will use
        let albums = repo.find_by_artist("Pink Floyd");
        assert_eq!(albums.len(), 1);
        let album_name_from_ui = albums[0].title.clone(); // this is what the UI sends back

        // Step 2: QuerySongsByAlbum — find_by_id using the title the UI provided
        let found = repo.find_by_id(&album_name_from_ui);
        assert!(found.is_some(), "find_by_id({:?}) returned None", album_name_from_ui);
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
        });

        assert_eq!(repo.find_by_artist("pink floyd").len(), 1);
        assert_eq!(repo.find_by_artist("PINK FLOYD").len(), 1);
        assert_eq!(repo.find_by_artist("Pink Floyd").len(), 1);
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

    // --- Genre normalization tests ---

    #[test]
    fn resolve_id3v1_numeric_genres() {
        use super::resolve_id3v1_genre;
        assert_eq!(resolve_id3v1_genre("(17)"), Some("Rock"));
        assert_eq!(resolve_id3v1_genre("(20)"), Some("Alternative"));
        assert_eq!(resolve_id3v1_genre("(35)"), Some("House"));
        assert_eq!(resolve_id3v1_genre("17"), Some("Rock"));
        assert_eq!(resolve_id3v1_genre("(999)"), None);
        assert_eq!(resolve_id3v1_genre("Rock"), None);
    }

    #[test]
    fn junk_genres_are_filtered() {
        use super::is_junk_genre;
        assert!(is_junk_genre("other"));
        assert!(is_junk_genre("unknown genre"));
        assert!(is_junk_genre("unknown"));
        assert!(!is_junk_genre("rock"));
        assert!(!is_junk_genre("jazz"));
    }

    #[test]
    fn genre_title_case() {
        use super::title_case_genre;
        assert_eq!(title_case_genre("progressive rock"), "Progressive Rock");
        assert_eq!(title_case_genre("hip-hop"), "Hip-hop");
        assert_eq!(title_case_genre("R&B"), "R&B");
    }

    #[test]
    fn find_all_by_genre_merges_case_variants() {
        let repo = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &repo,
            "a1", "Album One",   "Artist 1", Some("Electronic"),
            "a2", "Album Two",   "Artist 2", Some("electronic"),
            "a3", "Album Three", "Artist 3", Some("ELECTRONIC"),
            "a4", "Album Four",  "Artist 4", Some("Rock"),
            "a5", "Album Five",  "Artist 5", Some("rock")
        );
        let result = repo.find_all_by_genre(20);
        // All three "electronic" variants should merge into one group
        let electronic = result.iter().find(|(name, _)| name.to_lowercase() == "electronic");
        assert!(
            electronic.is_some(),
            "expected 'Electronic' group, got: {:?}",
            result.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
        assert_eq!(electronic.unwrap().1.len(), 3);
        // Both "Rock" variants should merge
        let rock = result.iter().find(|(name, _)| name.to_lowercase() == "rock");
        assert!(rock.is_some());
        assert_eq!(rock.unwrap().1.len(), 2);
        // Should be exactly 2 groups, not 5
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn find_all_by_genre_resolves_id3v1_codes() {
        let repo = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &repo,
            "a1", "Album One", "Artist 1", Some("(17)"),
            "a2", "Album Two", "Artist 2", Some("Rock")
        );
        let result = repo.find_all_by_genre(20);
        // "(17)" = Rock → should merge with "Rock"
        let rock = result.iter().find(|(name, _)| name.to_lowercase() == "rock");
        assert!(rock.is_some(), "expected 'Rock' group");
        assert_eq!(rock.unwrap().1.len(), 2);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn find_all_by_genre_filters_junk() {
        let repo = create_album_repo();
        #[rustfmt::skip]
        insert_albums!(
            &repo,
            "a1", "Album One",   "Artist 1", Some("Other"),
            "a2", "Album Two",   "Artist 2", Some("Other"),
            "a3", "Album Three", "Artist 3", Some("Unknown genre"),
            "a4", "Album Four",  "Artist 4", Some("Unknown genre"),
            "a5", "Album Five",  "Artist 5", Some("Jazz"),
            "a6", "Album Six",   "Artist 6", Some("Jazz")
        );
        let result = repo.find_all_by_genre(20);
        // "Other" and "Unknown genre" should be filtered out, only Jazz remains
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
        AlbumRepository::new(&ctx.db_dir)
    }
}
