use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::stat::PlayItemStatistics;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct Song {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<Duration>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub performer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,

    pub tags: HashMap<String, String>,

    pub file: String,

    pub file_date: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub statistics: Option<PlayItemStatistics>,
}

impl Song {
    #[must_use]
    pub fn to_json_string_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Song serialization failed!")
    }

    #[must_use]
    pub fn bytes_to_song(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    #[must_use]
    pub fn info_string(&self) -> Option<String> {
        let mut result = String::new();
        if let Some(artist) = self.artist.as_ref() {
            result.push_str(artist.as_str());
            result.push('-');
        }
        if let Some(album) = self.album.as_ref() {
            result.push_str(album.as_str());
            result.push('-');
        }
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        } else {
            result.push_str(self.file.as_str());
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
    #[must_use]
    pub fn get_title(&self) -> String {
        let mut result = String::new();
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        }
        if result.is_empty() {
            result.push_str(self.file.as_str());
        }
        result
    }
    pub fn get_file_name_without_path(&self) -> String {
        self.file.rsplit('/').next().unwrap_or(&self.file).to_owned()
    }

    /// Returns the track-level gain from `R128_TRACK_GAIN` or `REPLAYGAIN_TRACK_GAIN` tags.
    /// The gain is in dB and should be applied directly (as-is), not adjusted to any target LUFS.
    #[must_use]
    pub fn file_tag_track_gain(&self) -> Option<f64> {
        parse_r128_gain(&self.tags, "R128_TRACK_GAIN").or_else(|| parse_replaygain(&self.tags, "REPLAYGAIN_TRACK_GAIN"))
    }

    /// Returns the album-level gain from `R128_ALBUM_GAIN` or `REPLAYGAIN_ALBUM_GAIN` tags.
    /// The gain is in dB and should be applied directly (as-is), not adjusted to any target LUFS.
    #[must_use]
    pub fn file_tag_album_gain(&self) -> Option<f64> {
        parse_r128_gain(&self.tags, "R128_ALBUM_GAIN").or_else(|| parse_replaygain(&self.tags, "REPLAYGAIN_ALBUM_GAIN"))
    }

    #[must_use]
    pub fn all_text(&self) -> String {
        let mut result = String::new();
        if let Some(t) = self.title.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.artist.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.album.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.album_artist.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.genre.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.composer.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.performer.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.date.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }

        result
    }
}

/// Parse an EBU R128 gain tag stored as a Q7.8 fixed-point integer string (e.g. `"256"` → 1.0 dB).
fn parse_r128_gain(tags: &HashMap<String, String>, key: &str) -> Option<f64> {
    let value = tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)?;
    value.trim().parse::<i32>().ok().map(|n| f64::from(n) / 256.0)
}

/// Parse a `ReplayGain` text tag (e.g. `"+2.35 dB"`, `"-1.23 dB"`, or `"2.35"`).
fn parse_replaygain(tags: &HashMap<String, String>, key: &str) -> Option<f64> {
    let value = tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)?;
    value
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == ' ')
        .trim()
        .parse::<f64>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song_with_tags(pairs: &[(&str, &str)]) -> Song {
        let mut song = Song::default();
        for (k, v) in pairs {
            song.tags.insert((*k).to_owned(), (*v).to_owned());
        }
        song
    }

    // --- ReplayGain parsing ---

    #[test]
    fn replaygain_track_gain_with_db_suffix() {
        let song = song_with_tags(&[("REPLAYGAIN_TRACK_GAIN", "+3.14 dB")]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - 3.14).abs() < 1e-9);
    }

    #[test]
    fn replaygain_track_gain_negative() {
        let song = song_with_tags(&[("REPLAYGAIN_TRACK_GAIN", "-1.50 dB")]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - (-1.50)).abs() < 1e-9);
    }

    #[test]
    fn replaygain_track_gain_without_db_suffix() {
        let song = song_with_tags(&[("REPLAYGAIN_TRACK_GAIN", "2.35")]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - 2.35).abs() < 1e-9);
    }

    #[test]
    fn replaygain_key_lookup_is_case_insensitive() {
        let song = song_with_tags(&[("replaygain_track_gain", "+1.00 dB")]);
        assert!(song.file_tag_track_gain().is_some());
    }

    #[test]
    fn replaygain_album_gain_parsed() {
        let song = song_with_tags(&[("REPLAYGAIN_ALBUM_GAIN", "-1.50 dB")]);
        let gain = song.file_tag_album_gain().unwrap();
        assert!((gain - (-1.50)).abs() < 1e-9);
    }

    #[test]
    fn replaygain_returns_none_when_tag_absent() {
        let song = Song::default();
        assert!(song.file_tag_track_gain().is_none());
        assert!(song.file_tag_album_gain().is_none());
    }

    #[test]
    fn replaygain_returns_none_for_invalid_value() {
        let song = song_with_tags(&[("REPLAYGAIN_TRACK_GAIN", "not a number")]);
        assert!(song.file_tag_track_gain().is_none());
    }

    // --- R128 parsing ---

    #[test]
    fn r128_track_gain_positive() {
        // 512 / 256 = +2.0 dB
        let song = song_with_tags(&[("R128_TRACK_GAIN", "512")]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - 2.0).abs() < 1e-9);
    }

    #[test]
    fn r128_track_gain_negative() {
        // -256 / 256 = -1.0 dB
        let song = song_with_tags(&[("R128_TRACK_GAIN", "-256")]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn r128_album_gain_parsed() {
        // -256 / 256 = -1.0 dB
        let song = song_with_tags(&[("R128_ALBUM_GAIN", "-256")]);
        let gain = song.file_tag_album_gain().unwrap();
        assert!((gain - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn r128_key_lookup_is_case_insensitive() {
        let song = song_with_tags(&[("r128_track_gain", "256")]);
        assert!(song.file_tag_track_gain().is_some());
    }

    // --- Priority: R128 wins over ReplayGain when both present ---

    #[test]
    fn r128_takes_priority_over_replaygain_for_track() {
        let song = song_with_tags(&[
            ("R128_TRACK_GAIN", "256"), // +1.0 dB
            ("REPLAYGAIN_TRACK_GAIN", "+5.00 dB"),
        ]);
        let gain = song.file_tag_track_gain().unwrap();
        assert!((gain - 1.0).abs() < 1e-9, "R128 should take priority, got {gain}");
    }

    #[test]
    fn r128_takes_priority_over_replaygain_for_album() {
        let song = song_with_tags(&[
            ("R128_ALBUM_GAIN", "-256"), // -1.0 dB
            ("REPLAYGAIN_ALBUM_GAIN", "+5.00 dB"),
        ]);
        let gain = song.file_tag_album_gain().unwrap();
        assert!((gain - (-1.0)).abs() < 1e-9, "R128 should take priority, got {gain}");
    }

    // --- Track vs album independence ---

    #[test]
    fn track_and_album_gains_are_independent() {
        let song = song_with_tags(&[
            ("REPLAYGAIN_TRACK_GAIN", "+3.14 dB"),
            ("REPLAYGAIN_ALBUM_GAIN", "-1.50 dB"),
        ]);
        let track = song.file_tag_track_gain().unwrap();
        let album = song.file_tag_album_gain().unwrap();
        assert!((track - 3.14).abs() < 1e-9);
        assert!((album - (-1.50)).abs() < 1e-9);
    }
}
