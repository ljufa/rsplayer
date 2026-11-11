//! Tag → [`Song`] mapping: tags, duration and embedded artwork of a probed
//! format reader.
//!
//! The first tag for a `Song` field fills it. Every other tag is kept in
//! `Song.tags`, first non-empty value of a key wins: standard tags under Symphonia's name for them
//! (`SortArtist`, `Conductor`, `MusicBrainzTrackId`, `Lyrics`, ... the same
//! in every format), others under their raw key (`TXXX:<description>` for
//! ID3 user-defined frames). `ReplayGain` and album identity tags use the
//! fixed keys the player and `album_identity` read.
//!
//! Of several embedded pictures the front cover is preferred, then any
//! picture not marked as something else (back cover, artist photo, ...),
//! then the first.

use std::collections::HashSet;
use std::time::Duration;

use api_models::player::Song;

use crate::album_identity::{COMPILATION_TAG, MB_ALBUM_ID_TAG};
use symphonia::core::{
    formats::{FormatReader, TrackType},
    meta::{RawValue, StandardTag, StandardVisualKey, Tag, Visual},
};

pub struct AudioMetadataExtractor;

impl AudioMetadataExtractor {
    pub fn extract(format: &mut dyn FormatReader) -> (Song, Option<symphonia::core::meta::Visual>) {
        let mut song = Song::default();
        let mut image_data: Option<symphonia::core::meta::Visual> = None;

        if let Some(track) = format.default_track(TrackType::Audio)
            && let Some(num_frames) = track.num_frames
            && let Some(tb) = track.time_base
            && let Some(time) = tb.calc_time(symphonia::core::units::Timestamp::new(num_frames.cast_signed()))
        {
            song.time = Some(Duration::from_secs(time.as_secs().unsigned_abs()));
        }

        if let Some(metadata_rev) = format.metadata().skip_to_latest() {
            Self::fill_song_from_metadata(metadata_rev, &mut song, &mut image_data);
        }

        (song, image_data)
    }

    fn fill_song_from_metadata(
        metadata_rev: &symphonia::core::meta::MetadataRevision,
        song: &mut Song,
        image_data: &mut Option<symphonia::core::meta::Visual>,
    ) {
        // Kinds of standard tag that went into a `Song` field: their repeats
        // (a second ARTIST) are not kept under the kind's name.
        let mut filled: HashSet<String> = HashSet::new();
        for tag in &metadata_rev.media.tags {
            // Binary frames (ID3 PRIV, iTunes encoder params) would be stored
            // as hex text nobody can use.
            if matches!(tag.raw.value, RawValue::Binary(_)) {
                continue;
            }
            let Some(std_tag) = &tag.std else {
                Self::store(song, raw_key(tag), Self::tag_value_to_option(tag));
                continue;
            };
            let (kind, value) = std_name_value(std_tag);
            match Self::fill_field(std_tag, tag, song) {
                Fill::Filled => {
                    filled.insert(kind);
                }
                Fill::Taken if filled.contains(&kind) => {}
                Fill::Taken | Fill::NoField => {
                    let value = value.unwrap_or_else(|| Self::tag_value_to_option(tag));
                    Self::store(song, kind, value);
                }
            }
        }

        if image_data.is_none()
            && let Some(v) = Self::pick_visual(&metadata_rev.media.visuals)
        {
            *image_data = Some(v.clone());
        }
    }

    /// Puts a standard tag into its `Song` field, or under the fixed
    /// `Song.tags` key that other code reads (album identity, `ReplayGain`).
    fn fill_field(std_tag: &StandardTag, tag: &Tag, song: &mut Song) -> Fill {
        let slot = match std_tag {
            StandardTag::Album(_) => &mut song.album,
            StandardTag::AlbumArtist(_) => &mut song.album_artist,
            StandardTag::Artist(_) => &mut song.artist,
            StandardTag::Composer(_) => &mut song.composer,
            StandardTag::RecordingDate(_)
            | StandardTag::ReleaseDate(_)
            | StandardTag::RecordingYear(_)
            | StandardTag::ReleaseYear(_) => &mut song.date,
            StandardTag::DiscNumber(_) => &mut song.disc,
            StandardTag::Genre(_) => &mut song.genre,
            StandardTag::Label(_) => &mut song.label,
            StandardTag::Performer(_) => &mut song.performer,
            StandardTag::TrackNumber(_) => &mut song.track,
            StandardTag::TrackTitle(_) => &mut song.title,
            // Album identity (see `album_identity`), under fixed keys.
            StandardTag::CompilationFlag(flag) => {
                Self::store(song, COMPILATION_TAG.to_owned(), if *flag { "1" } else { "0" }.to_owned());
                return Fill::Filled;
            }
            StandardTag::MusicBrainzAlbumId(id) => {
                Self::store(song, MB_ALBUM_ID_TAG.to_owned(), id.trim().to_owned());
                return Fill::Filled;
            }
            // The keys `Song::file_tag_track_gain` and friends look up. The
            // raw key would be `TXXX` or an MP4 freeform atom in some formats.
            StandardTag::ReplayGainTrackGain(v) => return Self::fixed(song, "REPLAYGAIN_TRACK_GAIN", v),
            StandardTag::ReplayGainTrackPeak(v) => return Self::fixed(song, "REPLAYGAIN_TRACK_PEAK", v),
            StandardTag::ReplayGainAlbumGain(v) => return Self::fixed(song, "REPLAYGAIN_ALBUM_GAIN", v),
            StandardTag::ReplayGainAlbumPeak(v) => return Self::fixed(song, "REPLAYGAIN_ALBUM_PEAK", v),
            _ => return Fill::NoField,
        };
        if slot.is_some() {
            return Fill::Taken;
        }
        *slot = Some(Self::tag_value_to_option(tag));
        Fill::Filled
    }

    fn fixed(song: &mut Song, key: &str, value: &str) -> Fill {
        Self::store(song, key.to_owned(), value.trim().to_owned());
        Fill::Filled
    }

    /// Keeps a tag in `Song.tags`; the first non-empty value of a key wins.
    fn store(song: &mut Song, key: String, value: String) {
        if !value.is_empty() {
            song.tags.entry(key).or_insert(value);
        }
    }

    /// The picture best suited as cover art: front cover, else one without
    /// a declared usage (or `Other`), else the first.
    fn pick_visual(visuals: &[Visual]) -> Option<&Visual> {
        visuals
            .iter()
            .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
            .or_else(|| {
                visuals
                    .iter()
                    .find(|v| matches!(v.usage, None | Some(StandardVisualKey::Other)))
            })
            .or_else(|| visuals.first())
    }

    fn tag_value_to_option(tag: &symphonia::core::meta::Tag) -> String {
        tag.raw.value.to_string().trim().to_owned()
    }
}

enum Fill {
    /// The tag went into its `Song` field (or fixed key).
    Filled,
    /// It has a `Song` field, but an earlier tag already filled it.
    Taken,
    /// No `Song` field for this kind of tag.
    NoField,
}

/// The name of a standard tag's kind (`SortArtist`, `MusicBrainzTrackId`,
/// `TrackTotal`, ...), the same in every container format, and its value
/// when that is a number or flag. Text values are `None`: the caller takes
/// the raw value instead, which is the same text.
///
/// Symphonia has no name accessor, so this reads the derived `Debug` output
/// (`TrackTotal(12)`, `Conductor("...")`); `std_names_are_stable` pins it.
fn std_name_value(std_tag: &StandardTag) -> (String, Option<String>) {
    let debug = format!("{std_tag:?}");
    match debug.split_once('(') {
        Some((name, rest)) => {
            let payload = rest.strip_suffix(')').unwrap_or(rest);
            let value = (!payload.starts_with('"')).then(|| payload.to_owned());
            (name.to_owned(), value)
        }
        None => (debug, None),
    }
}

/// Key of a non-standard tag: the raw key, plus the description for ID3
/// user-defined frames (`TXXX:conductor`, `COMM:iTunNORM`), which all share
/// one frame id.
fn raw_key(tag: &Tag) -> String {
    let desc = tag
        .raw
        .sub_fields
        .iter()
        .flat_map(|s| s.iter())
        .find(|s| s.field == "DESCRIPTION")
        .map(|s| s.value.to_string())
        .filter(|d| !d.trim().is_empty());
    desc.map_or_else(|| tag.raw.key.clone(), |desc| format!("{}:{}", tag.raw.key, desc.trim()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn std_names_are_stable() {
        let s = |v: &str| Arc::new(v.to_owned());
        assert_eq!(std_name_value(&StandardTag::TrackTotal(12)), ("TrackTotal".into(), Some("12".into())));
        assert_eq!(std_name_value(&StandardTag::SortArtist(s("A, B"))), ("SortArtist".into(), None));
        assert_eq!(
            std_name_value(&StandardTag::MusicBrainzTrackId(s("x"))),
            ("MusicBrainzTrackId".into(), None)
        );
        assert_eq!(std_name_value(&StandardTag::CompilationFlag(true)).1.as_deref(), Some("true"));
        assert_eq!(std_name_value(&StandardTag::OriginalReleaseYear(1975)).1.as_deref(), Some("1975"));
    }
}
