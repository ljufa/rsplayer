//! Case-insensitive library match used by `SearchLibrary`.

use std::collections::BTreeMap;

use api_models::common::MetadataLibraryItem;
use api_models::player::Song;
use api_models::playlist::Album;

use crate::ports::album_repository::AlbumRepository;
use crate::ports::song_repository::SongRepository;

pub fn contains_ci(haystack: &str, needle: &str) -> bool {
    let n = needle.trim();
    !n.is_empty() && haystack.to_lowercase().contains(&n.to_lowercase())
}

pub fn song_matches(song: &Song, needle: &str) -> bool {
    contains_ci(&song.all_text(), needle) || contains_ci(&song.file, needle)
}

pub fn album_matches(album: &Album, needle: &str) -> bool {
    contains_ci(&album.title, needle) || album.artist.as_deref().is_some_and(|a| contains_ci(a, needle))
}

/// Artists (alpha), then albums, then songs. Streams the keyspaces and stops at `limit`.
pub fn search_library(albums: &dyn AlbumRepository, songs: &dyn SongRepository, term: &str, limit: usize) -> Vec<MetadataLibraryItem> {
    let needle = term.trim();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut artist_by_key = BTreeMap::new();
    let mut matched_albums = Vec::new();
    albums.visit_albums(&mut |alb| {
        if let Some(name) = alb.artist.as_ref()
            && contains_ci(name, needle)
        {
            artist_by_key.entry(name.to_lowercase()).or_insert_with(|| name.clone());
        }
        if matched_albums.len() < limit && album_matches(&alb, needle) {
            matched_albums.push(alb);
        }
        true
    });
    let mut out = Vec::with_capacity(limit.min(128));
    for name in artist_by_key.into_values() {
        if out.len() >= limit {
            return out;
        }
        out.push(MetadataLibraryItem::Artist { name });
    }
    for alb in matched_albums {
        if out.len() >= limit {
            return out;
        }
        out.push(MetadataLibraryItem::Album {
            name: alb.title,
            id: alb.id,
            artist: alb.artist.unwrap_or_default(),
            year: alb.released,
        });
    }
    songs.visit_songs(&mut |song| {
        if song_matches(&song, needle) {
            out.push(MetadataLibraryItem::SongItem(song));
            return out.len() < limit;
        }
        true
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_models::player::Song;

    fn song(title: &str, artist: &str, album: &str, file: &str) -> Song {
        Song {
            title: Some(title.into()),
            artist: Some(artist.into()),
            album: Some(album.into()),
            file: file.into(),
            ..Song::default()
        }
    }

    #[test]
    fn song_matches_any_tag_field() {
        let s = song("Blackbird", "The Beatles", "The White Album", "beatles/white/blackbird.flac");
        assert!(song_matches(&s, "black"));
        assert!(song_matches(&s, "beatles"));
        assert!(song_matches(&s, "white"));
        assert!(song_matches(&s, "FLAC"));
        assert!(!song_matches(&s, "radiohead"));
        assert!(!song_matches(&s, "   "));
    }

    #[test]
    fn album_matches_title_or_artist() {
        let a = Album {
            id: "x".into(),
            title: "OK Computer".into(),
            artist: Some("Radiohead".into()),
            added: chrono::Utc::now(),
            song_keys: Vec::new(),
            image_id: None,
            genre: None,
            label: None,
            released: None,
        };
        assert!(album_matches(&a, "computer"));
        assert!(album_matches(&a, "radio"));
        assert!(!album_matches(&a, "beatles"));
    }

    #[test]
    fn search_library_stops_song_scan_at_limit() {
        use crate::ports::fakes::{InMemoryAlbumRepository, InMemorySongRepository};
        use crate::ports::song_repository::SongRepository;
        use std::sync::atomic::Ordering;

        let albums = InMemoryAlbumRepository::default();
        let songs = InMemorySongRepository::default();
        for i in 0..8 {
            songs.save(&song("Hit", "X", "Y", &format!("f{i}.flac"))).expect("save");
        }
        let out = search_library(&albums, &songs, "hit", 2);
        assert_eq!(out.len(), 2);
        assert_eq!(songs.visits.load(Ordering::Relaxed), 2);
    }
}
