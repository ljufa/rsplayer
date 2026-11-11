//! Which album a song belongs to, and the album's artist.
//!
//! The album key is the `MusicBrainz` album id when the song carries one.
//! Otherwise it is the album folder, the album-artist tag and the album
//! title, all normalized (see `genre_utils::normalize_name`). The track
//! artist is deliberately not part of the key: a compilation without an
//! album-artist tag stays one album, while two releases with the same title
//! in different folders stay apart.
//!
//! The album folder is the song's directory, with disc folders folded into
//! the album: `Album/CD1`, `Album/Disc 2 - Bonus` and sibling folders such
//! as `Album (Disc 1)` / `Album (Disc 2)` all count as `Album`.

use api_models::player::Song;

use crate::genre_utils::normalize_name;

/// Display artist of albums with several track artists and no album artist.
pub const VARIOUS_ARTISTS: &str = "Various Artists";
/// `Song.tags` key of the compilation flag (`1` or `0`).
pub const COMPILATION_TAG: &str = "COMPILATION";
/// `Song.tags` key of the `MusicBrainz` album (release) id.
pub const MB_ALBUM_ID_TAG: &str = "MUSICBRAINZ_ALBUMID";

/// Raw tag names, besides the ones above, that carry the compilation flag
/// when a reader stores them unmapped.
const COMPILATION_RAW_TAGS: &[&str] = &["TCMP", "cpil", "itunescompilation"];
/// Raw tag names of the `MusicBrainz` album id (APE, Vorbis, ID3 TXXX, MP4).
const MB_ALBUM_ID_RAW_TAGS: &[&str] = &["musicbrainz album id", "txxx:musicbrainz album id", "----:com.apple.itunes:musicbrainz album id"];

/// Separators around a disc number: `CD-1`, `Disc_2`, `cd 3`.
const SEPARATORS: &[char] = &[' ', '-', '_', '.', '#'];

/// The album key of `song` with album title `album`, already trimmed and
/// not empty.
#[must_use]
pub fn album_key(song: &Song, album: &str) -> String {
    if let Some(mbid) = mb_album_id(song) {
        return format!("mb|{}", normalize_name(mbid));
    }
    let dir = normalize_name(&album_dir(&song.file));
    let album_artist = normalize_name(album_artist_tag(song).unwrap_or(""));
    format!("{dir}|{album_artist}|{}", normalize_name(album))
}

/// The album-artist tag, trimmed, `None` when missing or blank.
#[must_use]
pub fn album_artist_tag(song: &Song) -> Option<&str> {
    song.album_artist.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// The track artist, trimmed, `None` when missing or blank.
#[must_use]
pub fn track_artist(song: &Song) -> Option<&str> {
    song.artist.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// The artist an album shows after adding `song`, given the one it showed
/// so far: the album-artist tag wins; a compilation, or tracks by different
/// artists, show [`VARIOUS_ARTISTS`]; otherwise the track artist.
#[must_use]
pub fn album_display_artist(current: Option<&str>, song: &Song) -> Option<String> {
    if let Some(artist) = album_artist_tag(song) {
        return Some(artist.to_owned());
    }
    if is_compilation(song) {
        return Some(VARIOUS_ARTISTS.to_owned());
    }
    let Some(artist) = track_artist(song) else {
        return current.map(str::to_owned);
    };
    match current {
        Some(cur) if normalize_name(cur) != normalize_name(artist) => Some(VARIOUS_ARTISTS.to_owned()),
        Some(cur) => Some(cur.to_owned()),
        None => Some(artist.to_owned()),
    }
}

fn is_compilation(song: &Song) -> bool {
    tag(song, &[COMPILATION_TAG])
        .or_else(|| tag(song, COMPILATION_RAW_TAGS))
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn mb_album_id(song: &Song) -> Option<&str> {
    tag(song, &[MB_ALBUM_ID_TAG]).or_else(|| tag(song, MB_ALBUM_ID_RAW_TAGS))
}

/// First non-blank value of a tag named (case-insensitively) in `names`.
fn tag<'a>(song: &'a Song, names: &[&str]) -> Option<&'a str> {
    song.tags
        .iter()
        .filter(|(k, _)| names.iter().any(|n| k.eq_ignore_ascii_case(n)))
        .map(|(_, v)| v.trim())
        .find(|v| !v.is_empty())
}

/// The album folder of a library key (`Artist/Album/CD1/01.flac`,
/// `rs3:<id>/Album/01.flac`, `Album/Album.cue#CUE_01`), with disc folders
/// folded into the album folder.
#[must_use]
pub fn album_dir(file: &str) -> String {
    let Some((dir, _)) = file.rsplit_once(['/', '\\']) else {
        return String::new();
    };
    let (parent, last) = dir.rsplit_once(['/', '\\']).map_or(("", dir), |(p, l)| (p, l));
    let join = |name: &str| if parent.is_empty() { name.to_owned() } else { format!("{parent}/{name}") };
    if is_disc_dir(last) {
        return parent.to_owned();
    }
    let stripped = strip_disc_suffix(last);
    if stripped.len() == last.len() { dir.to_owned() } else { join(stripped) }
}

/// A folder holding one disc of an album: `CD1`, `cd 2`, `Disc 03`,
/// `Disk-1`, `CD1 - Bonus` (a disc marker and number at the start).
#[must_use]
pub fn is_disc_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(rest) = ["disc", "disk", "cd"].iter().find_map(|p| lower.strip_prefix(p)) else {
        return false;
    };
    let rest = rest.trim_start_matches(SEPARATORS);
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    digits > 0 && !rest[digits..].starts_with(|c: char| c.is_alphanumeric())
}

/// `name` without a trailing disc marker: `Album (Disc 1)`, `Album CD2`,
/// `Album [cd 3]`, `Album - Disc 4` all give `Album`. Unchanged when there
/// is none.
fn strip_disc_suffix(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    let s = lower.trim_end();
    let s = s.strip_suffix([')', ']']).unwrap_or(s);
    let no_digits = s.trim_end_matches(|c: char| c.is_ascii_digit());
    if no_digits.len() == s.len() {
        return name;
    }
    let s = no_digits.trim_end_matches(SEPARATORS);
    let Some(s) = ["disc", "disk", "cd"].iter().find_map(|m| s.strip_suffix(m)) else {
        return name;
    };
    // The marker must start a word: `Abacd1` is not a disc folder.
    if !s.ends_with(|c: char| SEPARATORS.contains(&c) || c == '(' || c == '[') {
        return name;
    }
    let s = s.trim_end_matches(|c: char| SEPARATORS.contains(&c) || c == '(' || c == '[');
    if s.is_empty() { name } else { &name[..s.len()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(file: &str, album_artist: Option<&str>, artist: &str) -> Song {
        Song {
            file: file.into(),
            album_artist: album_artist.map(Into::into),
            artist: Some(artist.into()),
            ..Default::default()
        }
    }

    #[test]
    fn compilation_without_album_artist_is_one_album() {
        let a = song("Hits 2024/01.flac", None, "Artist A");
        let b = song("Hits 2024/02.flac", None, "Artist B");
        assert_eq!(album_key(&a, "Hits 2024"), album_key(&b, "Hits 2024"));
        let shown = album_display_artist(None, &a);
        assert_eq!(shown.as_deref(), Some("Artist A"));
        assert_eq!(album_display_artist(shown.as_deref(), &b).as_deref(), Some(VARIOUS_ARTISTS));
    }

    #[test]
    fn same_title_in_different_folders_stays_apart() {
        let a = song("Queen/Greatest Hits/01.flac", Some("Queen"), "Queen");
        let b = song("Queen/Greatest Hits (2011 Remaster)/01.flac", Some("Queen"), "Queen");
        assert_ne!(album_key(&a, "Greatest Hits"), album_key(&b, "Greatest Hits"));
    }

    #[test]
    fn musicbrainz_id_wins_over_folders() {
        let mut a = song("X/CD1 extra/01.flac", None, "A");
        let mut b = song("Y/02.flac", None, "B");
        a.tags.insert(MB_ALBUM_ID_TAG.into(), "abc-123".into());
        b.tags.insert("MusicBrainz Album Id".into(), "ABC-123".into());
        assert_eq!(album_key(&a, "T"), album_key(&b, "T"));
        assert_eq!(album_key(&a, "T"), "mb|abc-123");
    }

    #[test]
    fn compilation_flag_shows_various_artists() {
        let mut a = song("C/01.mp3", None, "A");
        a.tags.insert("TCMP".into(), "1".into());
        assert_eq!(album_display_artist(None, &a).as_deref(), Some(VARIOUS_ARTISTS));
        let tagged = song("C/02.mp3", Some("DJ Mix"), "B");
        assert_eq!(album_display_artist(Some(VARIOUS_ARTISTS), &tagged).as_deref(), Some("DJ Mix"));
    }

    #[test]
    fn disc_folders_fold_into_the_album() {
        assert_eq!(album_dir("Artist/Album/CD1/01.flac"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album/Disc 2 - Bonus/01.flac"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album (Disc 1)/01.flac"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album [CD 2]/01.flac"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album - cd3/01.flac"), "Artist/Album");
        assert_eq!(album_dir("Album CD2/01.flac"), "Album");
        assert_eq!(album_dir("CD1/01.flac"), "");
        assert_eq!(album_dir("01.flac"), "");
        assert_eq!(album_dir("rs3:nas/Album/Album.cue#CUE_01"), "rs3:nas/Album");
        // Not disc folders.
        assert_eq!(album_dir("Artist/Abacd1/01.flac"), "Artist/Abacd1");
        assert_eq!(album_dir("Artist/Discography/01.flac"), "Artist/Discography");
        assert_eq!(album_dir("Artist/1999/01.flac"), "Artist/1999");
    }

    #[test]
    fn disc_dir_names() {
        for yes in ["CD1", "cd 2", "Disc 03", "disk-1", "DISC_4", "CD1 - Bonus"] {
            assert!(is_disc_dir(yes), "{yes}");
        }
        for no in ["CD", "Discography", "cd one", "Bonus", "cd2x"] {
            assert!(!is_disc_dir(no), "{no}");
        }
    }
}
