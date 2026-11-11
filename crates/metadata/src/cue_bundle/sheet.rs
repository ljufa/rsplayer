//! Cue sheet parsing.
//!
//! Handles what rippers actually write: a UTF-8 BOM, CRLF line ends,
//! quoted or bare values, `REM` comments carrying genre/date/ReplayGain, and
//! legacy 8-bit encodings (detected with `chardetng`). Times are kept in CD
//! frames (1/75 s) as written in the sheet.

use std::path::Path;

use anyhow::{Result, bail};

/// CD frames (sectors) per second: cue sheet times are `MM:SS:FF`.
pub const FRAMES_PER_SECOND: u64 = 75;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CueSheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    /// `REM` comments with upper-cased keys, e.g. `("DATE", "2002")`.
    pub rem: Vec<(String, String)>,
    pub files: Vec<CueFile>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CueFile {
    /// The file name as written in the sheet, relative to the sheet.
    pub name: String,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u32,
    /// `false` for data tracks of enhanced/mixed-mode CDs.
    pub is_audio: bool,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub songwriter: Option<String>,
    pub isrc: Option<String>,
    pub rem: Vec<(String, String)>,
    /// `INDEX 00` (pregap start), in CD frames from the start of the file.
    pub index00: Option<u64>,
    /// `INDEX 01` (track start), in CD frames from the start of the file.
    pub index01: Option<u64>,
}

/// Where one playable track lies in its audio file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSpan<'a> {
    /// Index into [`CueSheet::files`].
    pub file_idx: usize,
    pub track: &'a CueTrack,
    /// Start (`INDEX 01`) in CD frames.
    pub start: u64,
    /// End in CD frames: the next track's `INDEX 01` in the same file, or
    /// `None` for the last track of the file (it runs to the end). A gap
    /// before the next track (`INDEX 00`) stays with this track, as most
    /// players do.
    pub end: Option<u64>,
}

impl TrackSpan<'_> {
    /// Length in seconds, if the end is known.
    #[allow(clippy::cast_precision_loss)]
    pub fn duration_secs(&self) -> Option<f64> {
        self.end.map(|end| end.saturating_sub(self.start) as f64 / FRAMES_PER_SECOND as f64)
    }
}

impl CueSheet {
    /// Reads and parses a cue sheet file.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::parse(&decode_text(&bytes))
    }

    /// Parses cue sheet text. Fails when the sheet has no audio track.
    pub fn parse(text: &str) -> Result<Self> {
        let mut sheet = Self::default();
        // Commands before the first FILE that apply to every file.
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let (command, rest) = split_word(line);
            let command = command.to_ascii_uppercase();
            if command == "INDEX" {
                adopt_open_track(&mut sheet.files);
            }
            let track = sheet.files.last_mut().and_then(|f| f.tracks.last_mut());
            match command.as_str() {
                "FILE" => sheet.files.push(CueFile {
                    name: parse_file_name(rest),
                    tracks: Vec::new(),
                }),
                "TRACK" => {
                    let (number, kind) = split_word(rest);
                    let Ok(number) = number.parse::<u32>() else { continue };
                    // A TRACK before any FILE is malformed; give it an unnamed file.
                    if sheet.files.is_empty() {
                        sheet.files.push(CueFile::default());
                    }
                    if let Some(file) = sheet.files.last_mut() {
                        file.tracks.push(CueTrack {
                            number,
                            is_audio: kind.trim().eq_ignore_ascii_case("AUDIO"),
                            ..CueTrack::default()
                        });
                    }
                }
                "INDEX" => {
                    let (number, time) = split_word(rest);
                    if let (Some(track), Ok(number), Some(frames)) = (track, number.parse::<u32>(), parse_msf(time)) {
                        match number {
                            0 => track.index00 = Some(frames),
                            1 => track.index01 = Some(frames),
                            _ => {}
                        }
                    }
                }
                "TITLE" | "PERFORMER" | "SONGWRITER" => {
                    let value = non_empty(unquote(rest));
                    let slot = match (track, command.as_str()) {
                        (Some(t), "TITLE") => &mut t.title,
                        (Some(t), "PERFORMER") => &mut t.performer,
                        (Some(t), _) => &mut t.songwriter,
                        (None, "TITLE") => &mut sheet.title,
                        (None, "PERFORMER") => &mut sheet.performer,
                        (None, _) => &mut sheet.songwriter,
                    };
                    *slot = value;
                }
                "ISRC" => {
                    if let Some(track) = track {
                        track.isrc = non_empty(unquote(rest));
                    }
                }
                "REM" => {
                    let (key, value) = split_word(rest);
                    if let Some(value) = non_empty(unquote(value)) {
                        let entry = (key.to_ascii_uppercase(), value);
                        match track {
                            Some(track) => track.rem.push(entry),
                            None => sheet.rem.push(entry),
                        }
                    }
                }
                _ => {}
            }
        }
        if !sheet
            .files
            .iter()
            .flat_map(|f| &f.tracks)
            .any(|t| t.is_audio && t.index01.is_some())
        {
            bail!("cue sheet has no audio tracks");
        }
        Ok(sheet)
    }

    /// The value of a sheet-level `REM` comment.
    pub fn rem(&self, key: &str) -> Option<&str> {
        find_rem(&self.rem, key)
    }

    /// The playable (audio, with `INDEX 01`) tracks with their spans.
    pub fn spans(&self) -> Vec<TrackSpan<'_>> {
        let mut spans = Vec::new();
        for (file_idx, file) in self.files.iter().enumerate() {
            let tracks: Vec<(&CueTrack, u64)> = file
                .tracks
                .iter()
                .filter(|t| t.is_audio)
                .filter_map(|t| t.index01.map(|start| (t, start)))
                .collect();
            for (i, (track, start)) in tracks.iter().enumerate() {
                let end = tracks.get(i + 1).map(|(_, next)| *next).filter(|next| next > start);
                spans.push(TrackSpan {
                    file_idx,
                    track,
                    start: *start,
                    end,
                });
            }
        }
        spans
    }

    /// True when the sheet describes an album already split into one file per
    /// track (every file holds one track that starts at its first sample).
    /// Such files play fine on their own, so the sheet adds nothing.
    pub fn is_one_file_per_track(&self) -> bool {
        let spans = self.spans();
        spans.len() == self.files.len()
            && spans
                .iter()
                .enumerate()
                .all(|(i, s)| s.file_idx == i && s.start == 0 && s.end.is_none())
    }

    /// The span of track `number`.
    pub fn span(&self, number: u32) -> Option<TrackSpan<'_>> {
        self.spans().into_iter().find(|s| s.track.number == number)
    }
}

impl CueTrack {
    /// The value of a track-level `REM` comment.
    pub fn rem(&self, key: &str) -> Option<&str> {
        find_rem(&self.rem, key)
    }
}

/// EAC's "gaps appended to previous track" layout declares a TRACK (with its
/// `INDEX 00` pregap) at the end of one file and gives its `INDEX 01` after
/// the next `FILE` line. When an INDEX comes in a file that has no track yet,
/// move the previous file's last track, if it has not started, into it.
fn adopt_open_track(files: &mut [CueFile]) {
    let [.., prev, current] = files else { return };
    if !current.tracks.is_empty() || prev.tracks.last().is_none_or(|t| t.index01.is_some()) {
        return;
    }
    if let Some(mut track) = prev.tracks.pop() {
        // Its pregap is a position in the previous file.
        track.index00 = None;
        current.tracks.push(track);
    }
}

fn find_rem<'a>(rem: &'a [(String, String)], key: &str) -> Option<&'a str> {
    rem.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

/// Decodes cue sheet bytes: UTF-8 (BOM stripped) when valid, otherwise the
/// legacy encoding `chardetng` guesses (Windows-1250/1251/1252 and others).
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, chardetng::Utf8Detection::Deny);
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

/// Splits off the first whitespace-separated word.
fn split_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    s.split_once(char::is_whitespace)
        .map_or((s, ""), |(word, rest)| (word, rest.trim_start()))
}

/// Strips surrounding quotes from a value; bare values are taken whole.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .map_or(s, |inner| inner.rfind('"').map_or(inner, |end| &inner[..end]))
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// `FILE "name with spaces.flac" WAVE` or `FILE name.flac WAVE`.
fn parse_file_name(rest: &str) -> String {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('"') {
        return inner.rfind('"').map_or(inner, |end| &inner[..end]).to_owned();
    }
    // Bare name: everything but the trailing file type.
    rest.rsplit_once(char::is_whitespace)
        .map_or(rest, |(name, _)| name.trim_end())
        .to_owned()
}

/// `MM:SS:FF` to CD frames.
fn parse_msf(s: &str) -> Option<u64> {
    let mut parts = s.trim().split(':').map(|p| p.trim().parse::<u64>().ok());
    let (m, sec, f) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || sec >= 60 || f >= FRAMES_PER_SECOND {
        return None;
    }
    Some((m * 60 + sec) * FRAMES_PER_SECOND + f)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EAC: &str = "\u{FEFF}REM GENRE Alternative\r\n\
REM DATE 2002\r\n\
REM COMMENT \"ExactAudioCopy v1.6\"\r\n\
PERFORMER \"Darkwood Dub\"\r\n\
TITLE \"Zivot pocinje u 30oj\"\r\n\
REM COMPOSER \"\"\r\n\
FILE \"Darkwood Dub - Zivot pocinje u 30oj.flac\" WAVE\r\n\
  TRACK 01 AUDIO\r\n\
    TITLE \"Vrlog vira\"\r\n\
    PERFORMER \"Darkwood Dub\"\r\n\
    REM COMPOSER \"\"\r\n\
    INDEX 01 00:00:00\r\n\
  TRACK 02 AUDIO\r\n\
    TITLE \"Steppin' Out\"\r\n\
    INDEX 01 05:54:27\r\n\
  TRACK 03 AUDIO\r\n\
    TITLE \"Srebrna kap\"\r\n\
    INDEX 00 17:20:68\r\n\
    INDEX 01 17:21:00\r\n";

    #[test]
    fn parses_eac_sheet() {
        let sheet = CueSheet::parse(&decode_text(EAC.as_bytes())).unwrap();
        assert_eq!(sheet.title.as_deref(), Some("Zivot pocinje u 30oj"));
        assert_eq!(sheet.performer.as_deref(), Some("Darkwood Dub"));
        assert_eq!(sheet.rem("genre"), Some("Alternative"));
        assert_eq!(sheet.rem("DATE"), Some("2002"));
        assert_eq!(sheet.rem("COMPOSER"), None, "empty REM is dropped");
        assert_eq!(sheet.files.len(), 1);
        assert_eq!(sheet.files[0].name, "Darkwood Dub - Zivot pocinje u 30oj.flac");

        let spans = sheet.spans();
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].track.title.as_deref(), Some("Vrlog vira"));
        assert_eq!(spans[1].start, (5 * 60 + 54) * 75 + 27);
        // Track 2 ends at track 3's INDEX 01, so the pregap stays with track 2.
        assert_eq!(spans[1].end, Some((17 * 60 + 21) * 75));
        assert_eq!(spans[2].end, None);
        assert_eq!(spans[2].track.index00, Some((17 * 60 + 20) * 75 + 68));
    }

    #[test]
    fn bare_values_and_file_names() {
        let sheet = CueSheet::parse("TITLE Bare Title\nFILE CDImage.ape WAVE\nTRACK 1 AUDIO\nINDEX 01 00:00:00\n").unwrap();
        assert_eq!(sheet.title.as_deref(), Some("Bare Title"));
        assert_eq!(sheet.files[0].name, "CDImage.ape");
        assert_eq!(sheet.spans()[0].track.number, 1);
    }

    #[test]
    fn multiple_files_end_each_file_separately() {
        let text = "FILE \"a.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 01:00:00\n\
FILE \"b.flac\" WAVE\nTRACK 03 AUDIO\nINDEX 01 00:00:00\n";
        let sheet = CueSheet::parse(text).unwrap();
        let spans = sheet.spans();
        assert_eq!(spans.len(), 3);
        assert_eq!((spans[1].file_idx, spans[1].end), (0, None));
        assert_eq!((spans[2].file_idx, spans[2].start), (1, 0));
        assert_eq!(sheet.span(3).unwrap().file_idx, 1);
    }

    /// EAC "gaps appended to previous track": one file per track, but each
    /// TRACK is declared at the end of the previous file with its pregap
    /// (`INDEX 00`), and its `INDEX 01` opens the next file.
    const EAC_NONCOMPLIANT: &str = "FILE \"01 - A.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    INDEX 01 00:00:00\n\
  TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 00 03:48:65\nFILE \"02 - B.flac\" WAVE\n    INDEX 01 00:00:00\n\
  TRACK 03 AUDIO\n    TITLE \"C\"\n    INDEX 00 04:12:10\nFILE \"03 - C.flac\" WAVE\n    INDEX 01 00:00:00\n";

    #[test]
    fn eac_noncompliant_layout_is_one_file_per_track() {
        let sheet = CueSheet::parse(EAC_NONCOMPLIANT).unwrap();
        let spans = sheet.spans();
        let got: Vec<_> = spans.iter().map(|s| (s.track.number, s.file_idx, s.start, s.end)).collect();
        assert_eq!(got, vec![(1, 0, 0, None), (2, 1, 0, None), (3, 2, 0, None)]);
        assert_eq!(sheet.span(2).unwrap().track.title.as_deref(), Some("B"));
        assert!(sheet.is_one_file_per_track());
    }

    #[test]
    fn one_file_per_track_detection() {
        let compliant = "FILE \"1.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n\
FILE \"2.flac\" WAVE\nTRACK 02 AUDIO\nINDEX 00 00:00:00\nINDEX 01 00:02:00\n";
        // Compliant layout with the pregap at the start of the file: the track
        // does not start at 0, so the file must still be cut.
        assert!(!CueSheet::parse(compliant).unwrap().is_one_file_per_track());
        let plain = "FILE \"1.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n\
FILE \"2.flac\" WAVE\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n";
        assert!(CueSheet::parse(plain).unwrap().is_one_file_per_track());
        let single = "FILE \"1.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n";
        assert!(CueSheet::parse(single).unwrap().is_one_file_per_track());
        assert!(!CueSheet::parse(&decode_text(EAC.as_bytes())).unwrap().is_one_file_per_track());
    }

    #[test]
    fn skips_data_tracks_and_rejects_empty_sheets() {
        let text = "FILE \"x.bin\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\n";
        assert!(CueSheet::parse(text).is_err());
        assert!(CueSheet::parse("REM nothing here\n").is_err());
    }

    #[test]
    fn decodes_windows_1250() {
        // "Đorđe Balašević - Računajte na nas" in Windows-1250.
        let (bytes, _, _) = encoding_rs::WINDOWS_1250
            .encode("TITLE \"Đorđe Balašević - Računajte na nas\"\nFILE \"a.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n");
        assert!(std::str::from_utf8(&bytes).is_err());
        let sheet = CueSheet::parse(&decode_text(&bytes)).unwrap();
        assert_eq!(sheet.title.as_deref(), Some("Đorđe Balašević - Računajte na nas"));
    }

    #[test]
    fn rejects_bad_times() {
        assert_eq!(parse_msf("01:02:03"), Some((60 + 2) * 75 + 3));
        assert_eq!(parse_msf("00:60:00"), None);
        assert_eq!(parse_msf("00:00:75"), None);
        assert_eq!(parse_msf("1:2"), None);
    }
}
