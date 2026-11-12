//! Opaque Subsonic ids for library entities.
//!
//! An id is a type prefix and the entity's library key in unpadded
//! base64url: `so-` songs (file path, `rs3:` key or cue track key), `al-`
//! albums (album key), `ar-` artists (album artist name), `pl-` saved
//! playlists (name) and `ca-` cover art (artwork image id). The dynamic
//! playlists have fixed ids. Ids are URL-safe and stable as long as the key
//! is.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

pub const MOST_PLAYED_PLAYLIST: &str = "pl-~most_played";
pub const LIKED_PLAYLIST: &str = "pl-~liked";
/// The single music folder.
pub const MUSIC_FOLDER_ID: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Song,
    Album,
    Artist,
    Playlist,
    CoverArt,
}

impl Kind {
    const fn prefix(self) -> &'static str {
        match self {
            Self::Song => "so-",
            Self::Album => "al-",
            Self::Artist => "ar-",
            Self::Playlist => "pl-",
            Self::CoverArt => "ca-",
        }
    }

    const ALL: [Self; 5] = [Self::Song, Self::Album, Self::Artist, Self::Playlist, Self::CoverArt];
}

pub fn encode(kind: Kind, key: &str) -> String {
    format!("{}{}", kind.prefix(), URL_SAFE_NO_PAD.encode(key.as_bytes()))
}

pub fn song(key: &str) -> String {
    encode(Kind::Song, key)
}
pub fn album(key: &str) -> String {
    encode(Kind::Album, key)
}
pub fn artist(name: &str) -> String {
    encode(Kind::Artist, name)
}
pub fn playlist(name: &str) -> String {
    encode(Kind::Playlist, name)
}
pub fn cover_art(image_id: &str) -> String {
    encode(Kind::CoverArt, image_id)
}

/// The kind and key of `id`, `None` for anything this server did not hand out.
pub fn decode(id: &str) -> Option<(Kind, String)> {
    let kind = Kind::ALL.into_iter().find(|k| id.starts_with(k.prefix()))?;
    let bytes = URL_SAFE_NO_PAD.decode(&id[kind.prefix().len()..]).ok()?;
    let key = String::from_utf8(bytes).ok()?;
    (!key.is_empty()).then_some((kind, key))
}

/// The key of `id` when it is of `kind`.
pub fn decode_as(kind: Kind, id: &str) -> Option<String> {
    decode(id).filter(|(k, _)| *k == kind).map(|(_, key)| key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for key in [
            "Artist/Album (2001)/01 - Intro.flac",
            "rs3:nas/Ärzte/Album/02.mp3",
            "Album/Album.cue#CUE_03",
            "mb|1234-abcd",
            "a?b&c=d/e+f",
        ] {
            let id = song(key);
            assert!(id.starts_with("so-"));
            assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{id}");
            assert_eq!(decode(&id), Some((Kind::Song, key.to_string())));
        }
        assert_eq!(decode_as(Kind::Album, &album("x|y|z")).as_deref(), Some("x|y|z"));
        assert_eq!(decode_as(Kind::Album, &artist("x")), None);
        assert_eq!(decode_as(Kind::Artist, &artist("Björk")).as_deref(), Some("Björk"));
        assert_eq!(decode_as(Kind::CoverArt, &cover_art("123")).as_deref(), Some("123"));
    }

    #[test]
    fn foreign_ids_are_rejected() {
        assert_eq!(decode("123"), None);
        assert_eq!(decode("so-"), None);
        assert_eq!(decode("so-!!"), None);
        assert_eq!(decode(MOST_PLAYED_PLAYLIST), None);
    }
}
