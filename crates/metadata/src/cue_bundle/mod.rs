//! Cue sheets: albums ripped to one audio file plus a `.cue` sheet (or with
//! the sheet embedded in the file's `CUESHEET` tag) appear as separate
//! tracks.
//!
//! Every track is a virtual song keyed `{base}#CUE_{nn}` ([`CUE_TRACK_MARKER`]),
//! where `base` is the `.cue` file for an external sheet or the audio file
//! for an embedded one, and `nn` the track number. The key holds no offsets:
//! [`open_track`] re-reads the (tiny) sheet to find the track's span, so keys
//! stay stable when a sheet is fixed and the library rescanned.
//!
//! Playback goes through [`CueTrackReader`] (seeks to the track, stops at
//! its end, timestamps from 0) and [`CueTrimDecoder`] (cuts the boundary
//! packets to the exact sample). Browser playback fetches tracks as WAV
//! ranges decoded on demand ([`wav::WavTrack`]).

pub mod decoder;
pub mod reader;
pub mod sheet;
pub mod wav;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use api_models::player::Song;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

pub use decoder::CueTrimDecoder;
pub use reader::CueTrackReader;
pub use sheet::{CueSheet, FRAMES_PER_SECOND, TrackSpan};

pub const CUE_TRACK_MARKER: &str = "#CUE_";

/// Audio formats a cue sheet may point to.
pub const CUE_AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "ape", "wav", "aiff", "aif", "m4a", "mp3", "ogg", "oga", "opus", "mka", "caf",
];

/// Tag holding an embedded cue sheet (FLAC/APE/WavPack images).
const CUESHEET_TAG: &str = "CUESHEET";

/// Key of track `number` of the sheet at (or embedded in) `base_key`.
pub fn track_key(base_key: &str, number: u32) -> String {
    format!("{base_key}{CUE_TRACK_MARKER}{number:02}")
}

/// Splits a cue track key into its base key and track number.
pub fn split_track_key(key: &str) -> Option<(&str, u32)> {
    let pos = key.rfind(CUE_TRACK_MARKER)?;
    let number = key[pos + CUE_TRACK_MARKER.len()..].parse().ok()?;
    Some((&key[..pos], number))
}

pub fn is_cue_key(key: &str) -> bool {
    split_track_key(key).is_some()
}

pub fn is_cue_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"))
}

fn has_audio_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CUE_AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Finds the audio file a sheet's `FILE` line names, relative to the sheet.
///
/// Rippers often convert the image after writing the sheet, so besides the
/// exact name (in any letter case) it accepts the same name with another
/// audio extension (`FILE "CD.wav"` next to `CD.flac`), and finally the
/// sheet's own name without `.cue` (`CDImage.ape.cue` → `CDImage.ape`,
/// `Album.cue` → `Album.flac`).
pub fn resolve_audio_file(cue_path: &Path, file_name: &str) -> Option<PathBuf> {
    let dir = cue_path.parent()?;
    // Windows rippers may write backslashes or an absolute path.
    let normalized = file_name.replace('\\', "/");
    let base_name = normalized.rsplit('/').next().unwrap_or(&normalized).to_owned();

    let candidate = dir.join(&normalized);
    if !normalized.is_empty() && candidate.is_file() && has_audio_extension(&candidate) {
        return Some(candidate);
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && has_audio_extension(p))
        .collect();
    let name_of = |p: &Path| p.file_name().and_then(|n| n.to_str()).map(str::to_owned);
    let stem_of = |p: &Path| p.file_stem().and_then(|n| n.to_str()).map(str::to_owned);

    let wanted = Path::new(&base_name);
    if let Some(found) = entries
        .iter()
        .find(|p| name_of(p).is_some_and(|n| n.eq_ignore_ascii_case(&base_name)))
    {
        return Some(found.clone());
    }
    if let Some(stem) = stem_of(wanted)
        && let Some(found) = entries.iter().find(|p| stem_of(p).is_some_and(|s| s.eq_ignore_ascii_case(&stem)))
    {
        return Some(found.clone());
    }
    // The sheet's own stem: "Album.cue" -> "Album.*", "CDImage.ape.cue" -> "CDImage.ape".
    let cue_stem = stem_of(cue_path)?;
    entries
        .iter()
        .find(|p| name_of(p).is_some_and(|n| n.eq_ignore_ascii_case(&cue_stem)))
        .or_else(|| {
            entries
                .iter()
                .find(|p| stem_of(p).is_some_and(|s| s.eq_ignore_ascii_case(&cue_stem)))
        })
        .cloned()
}

/// The audio files of `sheet` (at `cue_path`) that exist, by file index.
pub fn resolve_audio_files(cue_path: &Path, sheet: &CueSheet) -> Vec<Option<PathBuf>> {
    sheet.files.iter().map(|f| resolve_audio_file(cue_path, &f.name)).collect()
}

/// Opens an audio file for reading packets. APE uses its streaming reader
/// (the probe path would read the whole file); everything else is probed.
pub fn open_audio(path: &Path) -> Result<Box<dyn FormatReader>> {
    let file = std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ape")) {
        // Large buffer: APE frames are big and libraries often sit on NFS/SMB.
        let reader = std::io::BufReader::with_capacity(1024 * 1024, file);
        return Ok(Box::new(crate::ape_bundle::ApeReader::try_new_from_reader(reader)?));
    }
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(&ext.to_lowercase());
    }
    let mss = MediaSourceStream::new(Box::new(file), symphonia::core::io::MediaSourceStreamOptions::default());
    Ok(crate::build_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())?)
}

/// The cue sheet embedded in a file's tags, if any.
pub fn embedded_sheet<S: std::hash::BuildHasher>(tags: &HashMap<String, String, S>) -> Option<CueSheet> {
    let text = tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(CUESHEET_TAG)).map(|(_, v)| v)?;
    let sheet = CueSheet::parse(text).ok()?;
    // A sheet embedded in one file describes that file only; one track that
    // spans the whole file is just the file.
    (sheet.files.len() <= 1 && !sheet.is_one_file_per_track()).then_some(sheet)
}

fn embedded_sheet_of(reader: &mut dyn FormatReader) -> Option<CueSheet> {
    let metadata = reader.metadata();
    let revision = metadata.current()?;
    let tags: HashMap<String, String> = revision
        .media
        .tags
        .iter()
        .filter(|t| t.raw.key.eq_ignore_ascii_case(CUESHEET_TAG))
        .map(|t| (t.raw.key.clone(), t.raw.value.to_string()))
        .collect();
    embedded_sheet(&tags)
}

/// Opens cue track `key` for playback or analysis: finds the sheet and the
/// audio file in `music_dirs` and positions a [`CueTrackReader`] on the track.
pub fn open_track(key: &str, music_dirs: &[String]) -> Result<CueTrackReader> {
    let (base, number) = split_track_key(key).context("not a cue track key")?;
    let base_path = music_dirs
        .iter()
        .map(|dir| Path::new(dir).join(base))
        .find(|p| p.is_file())
        .with_context(|| format!("cannot find {base}"))?;

    let (reader, sheet) = if is_cue_file(&base_path) {
        let sheet = CueSheet::read(&base_path)?;
        let span = sheet.span(number).with_context(|| format!("no track {number} in {base}"))?;
        let audio =
            resolve_audio_file(&base_path, &sheet.files[span.file_idx].name).with_context(|| format!("audio file of {base} not found"))?;
        (open_audio(&audio)?, sheet)
    } else {
        let mut reader = open_audio(&base_path)?;
        let sheet = embedded_sheet_of(&mut *reader).with_context(|| format!("no cue sheet in {base}"))?;
        (reader, sheet)
    };
    let span = sheet.span(number).with_context(|| format!("no track {number} in {base}"))?;

    let track = reader.default_track(TrackType::Audio).context("no audio track")?;
    let track_id = track.id;
    let rate = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .and_then(|p| p.sample_rate)
        .context("unknown sample rate")?;
    let tb = track.time_base;
    let start = frames_to_ts(span.start, rate, tb);
    let end = span.end.map(|e| frames_to_ts(e, rate, tb));
    if end.is_some_and(|e| e <= start) {
        bail!("track {number} of {base} is empty");
    }
    Ok(CueTrackReader::new(reader, track_id, start, end)?)
}

/// CD frames to a track timestamp: exact integer math for the usual
/// `1/sample_rate` timebase.
fn frames_to_ts(frames: u64, rate: u32, tb: Option<TimeBase>) -> u64 {
    match tb {
        Some(tb) if !(tb.numer.get() == 1 && tb.denom.get() == rate) => {
            let time = Time::try_new(
                (frames / FRAMES_PER_SECOND).cast_signed(),
                u32::try_from((frames % FRAMES_PER_SECOND) * 1_000_000_000 / FRAMES_PER_SECOND).unwrap_or(0),
            )
            .unwrap_or(Time::ZERO);
            tb.calc_timestamp(time).map_or(0, |ts: Timestamp| ts.get().max(0).unsigned_abs())
        }
        _ => frames * u64::from(rate) / FRAMES_PER_SECOND,
    }
}

/// Tags of the whole file that would be wrong on a single track.
fn is_whole_file_tag(key: &str) -> bool {
    const DROPPED: &[&str] = &[
        CUESHEET_TAG,
        "REPLAYGAIN_TRACK_GAIN",
        "REPLAYGAIN_TRACK_PEAK",
        "R128_TRACK_GAIN",
        "TRACKNUMBER",
        "TRACKTOTAL",
        "TOTALTRACKS",
        "ISRC",
    ];
    DROPPED.iter().any(|d| d.eq_ignore_ascii_case(key))
}

/// The library songs for a sheet's tracks.
///
/// `base_key` is the sheet's key (or the audio file's, for an embedded
/// sheet); `file_songs` holds, per [`CueSheet::files`] entry, the song read
/// from that audio file (its tags, artwork and duration), `None` when the
/// file is missing. The sheet wins over the file's tags: a single-file rip
/// usually carries the tags of one track (or none) for the whole album.
pub fn track_songs(sheet: &CueSheet, base_key: &str, file_songs: &[Option<Song>]) -> Vec<Song> {
    let album_composer = sheet.songwriter.clone().or_else(|| sheet.rem("COMPOSER").map(str::to_owned));
    sheet
        .spans()
        .into_iter()
        .filter_map(|span| {
            let file_song = file_songs.get(span.file_idx)?.as_ref()?;
            let track = span.track;
            #[allow(clippy::cast_precision_loss)]
            let start_secs = span.start as f64 / FRAMES_PER_SECOND as f64;
            let secs = span
                .duration_secs()
                .or_else(|| file_song.time.map(|t| (t.as_secs_f64() - start_secs).max(0.0)));

            let mut tags: HashMap<String, String> = file_song
                .tags
                .iter()
                .filter(|(k, _)| !is_whole_file_tag(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (k, v) in sheet.rem.iter().chain(&track.rem) {
                if k.starts_with("REPLAYGAIN_") {
                    tags.insert(k.clone(), v.clone());
                }
            }
            if let Some(isrc) = &track.isrc {
                tags.insert("ISRC".to_owned(), isrc.clone());
            }

            let artist = track
                .performer
                .clone()
                .or_else(|| sheet.performer.clone())
                .or_else(|| file_song.artist.clone());
            Some(Song {
                title: Some(track.title.clone().unwrap_or_else(|| format!("Track {}", track.number))),
                album: sheet.title.clone().or_else(|| file_song.album.clone()),
                // No fallback to the track performer: a compilation sheet
                // without an album PERFORMER must stay one album.
                album_artist: sheet.performer.clone().or_else(|| file_song.album_artist.clone()),
                artist,
                genre: sheet.rem("GENRE").map(str::to_owned).or_else(|| file_song.genre.clone()),
                date: sheet.rem("DATE").map(str::to_owned).or_else(|| file_song.date.clone()),
                track: Some(track.number.to_string()),
                disc: sheet.rem("DISCNUMBER").map(str::to_owned).or_else(|| file_song.disc.clone()),
                composer: track
                    .songwriter
                    .clone()
                    .or_else(|| track.rem("COMPOSER").map(str::to_owned))
                    .or_else(|| album_composer.clone())
                    .or_else(|| file_song.composer.clone()),
                performer: file_song.performer.clone(),
                label: file_song.label.clone(),
                time: secs.map(Duration::from_secs_f64),
                image_id: file_song.image_id.clone(),
                image_url: file_song.image_url.clone(),
                tags,
                file: track_key(base_key, track.number),
                file_date: file_song.file_date,
                ..Song::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        let key = track_key("Rock/Album/CD.cue", 7);
        assert_eq!(key, "Rock/Album/CD.cue#CUE_07");
        assert_eq!(split_track_key(&key), Some(("Rock/Album/CD.cue", 7)));
        assert_eq!(split_track_key("Rock/Album/01.flac"), None);
        assert!(is_cue_key("a.flac#CUE_12"));
    }

    #[test]
    fn frame_math_is_exact_for_common_rates() {
        assert_eq!(frames_to_ts(75, 44_100, None), 44_100);
        assert_eq!(frames_to_ts(1, 44_100, None), 588);
        assert_eq!(frames_to_ts(1, 96_000, None), 1_280);
        assert_eq!(
            frames_to_ts((5 * 60 + 54) * 75 + 27, 44_100, TimeBase::try_from_recip(44_100)),
            15_627_276
        );
    }

    #[test]
    fn sheet_wins_over_whole_file_tags() {
        let sheet = CueSheet::parse(
            "REM GENRE Alternative\nREM DATE 2002\nREM REPLAYGAIN_ALBUM_GAIN -7.50 dB\nPERFORMER \"Darkwood Dub\"\nTITLE \"Album\"\n\
FILE \"a.flac\" WAVE\nTRACK 01 AUDIO\nTITLE \"One\"\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nTITLE \"Two\"\nPERFORMER \"Guest\"\n\
REM REPLAYGAIN_TRACK_GAIN -6.00 dB\nINDEX 01 01:00:00\n",
        )
        .unwrap();
        let file_song = Song {
            title: Some("Steppin' Out".into()),
            track: Some("02".into()),
            artist: Some("Darkwood Dub".into()),
            composer: Some("Somebody".into()),
            time: Some(Duration::from_secs(150)),
            image_id: Some("art".into()),
            tags: HashMap::from([
                ("REPLAYGAIN_TRACK_GAIN".to_owned(), "-9.00 dB".to_owned()),
                ("CUESHEET".to_owned(), "...".to_owned()),
                ("ENCODER".to_owned(), "flac".to_owned()),
            ]),
            ..Song::default()
        };
        let songs = track_songs(&sheet, "x/Album.cue", &[Some(file_song)]);
        assert_eq!(songs.len(), 2);
        let (one, two) = (&songs[0], &songs[1]);
        assert_eq!(one.file, "x/Album.cue#CUE_01");
        assert_eq!(one.title.as_deref(), Some("One"));
        assert_eq!(one.track.as_deref(), Some("1"));
        assert_eq!(one.time, Some(Duration::from_secs(60)));
        assert_eq!(one.album.as_deref(), Some("Album"));
        assert_eq!(one.genre.as_deref(), Some("Alternative"));
        assert_eq!(one.composer.as_deref(), Some("Somebody"));
        assert_eq!(one.image_id.as_deref(), Some("art"));
        assert_eq!(one.file_tag_track_gain(), None, "the file's whole-album track gain is dropped");
        assert_eq!(one.file_tag_album_gain(), Some(-7.5));
        assert!(!one.tags.contains_key("CUESHEET"));
        assert!(one.tags.contains_key("ENCODER"));

        assert_eq!(two.artist.as_deref(), Some("Guest"));
        assert_eq!(two.album_artist.as_deref(), Some("Darkwood Dub"));
        assert_eq!(two.time, Some(Duration::from_secs(90)), "last track runs to the end of the file");
        assert_eq!(two.file_tag_track_gain(), Some(-6.0));
    }

    #[test]
    fn missing_audio_file_yields_no_songs() {
        let sheet = CueSheet::parse("FILE \"a.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n").unwrap();
        assert_eq!(track_songs(&sheet, "a.cue", &[None]).len(), 0);
    }
}
