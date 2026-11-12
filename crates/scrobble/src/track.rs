//! [`ScrobbleTrack`]: what is sent about a song, taken from a library [`Song`].

use std::time::Duration;

use api_models::player::Song;
use serde::{Deserialize, Serialize};

/// `Song.tags` keys of the `MusicBrainz` ids, as the library scanner stores
/// them (Symphonia's standard tag names, plus the fixed album id key).
/// `MusicBrainzTrackId` is what Picard writes as the recording id in Vorbis
/// comments and MP4 (`MUSICBRAINZ_TRACKID`, `MusicBrainz Track Id`).
const RECORDING_ID_TAGS: &[&str] = &["MusicBrainzRecordingId", "MusicBrainzTrackId"];
const RELEASE_ID_TAGS: &[&str] = &["MUSICBRAINZ_ALBUMID", "MusicBrainzAlbumId"];
const RELEASE_TRACK_ID_TAGS: &[&str] = &["MusicBrainzReleaseTrackId"];
const ARTIST_ID_TAGS: &[&str] = &["MusicBrainzArtistId"];

/// Track metadata sent to the services; also what the retry queue stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScrobbleTrack {
    pub artist: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_mbid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_mbid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_track_mbid: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artist_mbids: Vec<String>,
}

/// A finished listen: the track and when it started (unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listen {
    pub track: ScrobbleTrack,
    pub listened_at: i64,
}

impl ScrobbleTrack {
    /// The track to report for `song`, or `None` when it must not be
    /// scrobbled: no artist or title, a stream URL (radio, podcast episode)
    /// or a podcast episode queued as a song.
    #[must_use]
    pub fn from_song(song: &Song) -> Option<Self> {
        if is_stream(&song.file) || song.podcast_episode_id().is_some() {
            return None;
        }
        let artist = non_empty(song.artist.as_deref())?;
        let title = non_empty(song.title.as_deref())?;
        Some(Self {
            artist,
            title,
            album: non_empty(song.album.as_deref()),
            album_artist: non_empty(song.album_artist.as_deref()),
            duration_secs: song.time.map(|d| d.as_secs()).filter(|s| *s > 0),
            track_number: song.track.as_deref().and_then(parse_track_number),
            recording_mbid: first_mbid(song, RECORDING_ID_TAGS),
            release_mbid: first_mbid(song, RELEASE_ID_TAGS),
            release_track_mbid: first_mbid(song, RELEASE_TRACK_ID_TAGS),
            artist_mbids: tag(song, ARTIST_ID_TAGS).map(mbids).unwrap_or_default(),
        })
    }

    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.duration_secs.map(Duration::from_secs)
    }
}

fn is_stream(file: &str) -> bool {
    let lower = file.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// `"3"`, `"03"`, `"3/12"` give 3.
fn parse_track_number(value: &str) -> Option<u32> {
    value.split('/').next()?.trim().parse().ok().filter(|n| *n > 0)
}

fn tag<'a>(song: &'a Song, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| song.tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str()))
}

fn first_mbid(song: &Song, keys: &[&str]) -> Option<String> {
    tag(song, keys).and_then(|v| mbids(v).into_iter().next())
}

/// Valid `MusicBrainz` ids in a tag value; several are separated by `;`,
/// `/` or `,` depending on the tagger. Invalid ones are dropped (`ListenBrainz`
/// rejects a whole listen with a malformed id).
fn mbids(value: &str) -> Vec<String> {
    value
        .split([';', '/', ',', '\0'])
        .map(str::trim)
        .filter(|id| is_mbid(id))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// A UUID in its 8-4-4-4-12 hex form.
fn is_mbid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, len)| g.len() == len && g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MBID: &str = "0b1d6b7c-1234-4cde-8f00-aabbccddeeff";

    fn song() -> Song {
        Song {
            artist: Some(" Artist ".into()),
            title: Some("Title".into()),
            album: Some("Album".into()),
            track: Some("3/12".into()),
            time: Some(Duration::from_secs(200)),
            file: "music/a.flac".into(),
            ..Default::default()
        }
    }

    #[test]
    fn library_song_becomes_track() {
        let mut s = song();
        s.tags.insert("MusicBrainzTrackId".into(), MBID.to_uppercase());
        s.tags.insert("MUSICBRAINZ_ALBUMID".into(), MBID.into());
        s.tags.insert("MusicBrainzArtistId".into(), format!("{MBID}; not-an-id; {MBID}"));
        let t = ScrobbleTrack::from_song(&s).unwrap();
        assert_eq!(t.artist, "Artist");
        assert_eq!(t.track_number, Some(3));
        assert_eq!(t.duration_secs, Some(200));
        assert_eq!(t.recording_mbid.as_deref(), Some(MBID));
        assert_eq!(t.release_mbid.as_deref(), Some(MBID));
        assert_eq!(t.artist_mbids, vec![MBID.to_string(), MBID.to_string()]);
        assert_eq!(t.release_track_mbid, None);
    }

    #[test]
    fn needs_artist_and_title() {
        let mut s = song();
        s.artist = Some("  ".into());
        assert!(ScrobbleTrack::from_song(&s).is_none());
        let mut s = song();
        s.title = None;
        assert!(ScrobbleTrack::from_song(&s).is_none());
    }

    #[test]
    fn streams_and_podcast_episodes_are_not_tracks() {
        let mut s = song();
        s.file = "https://radio.example/stream".into();
        assert!(ScrobbleTrack::from_song(&s).is_none());
        let mut s = song();
        s.tags.insert(api_models::podcast::EPISODE_ID_TAG.into(), "e1".into());
        assert!(ScrobbleTrack::from_song(&s).is_none());
        // Remote (S3) library songs are library songs.
        let mut s = song();
        s.file = "rs3:nas/a.flac".into();
        assert!(ScrobbleTrack::from_song(&s).is_some());
    }

    #[test]
    fn invalid_mbids_are_dropped() {
        let mut s = song();
        s.tags.insert("MusicBrainzRecordingId".into(), "garbage".into());
        assert_eq!(ScrobbleTrack::from_song(&s).unwrap().recording_mbid, None);
    }
}
