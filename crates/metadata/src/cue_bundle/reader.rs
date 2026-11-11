//! [`CueTrackReader`]: a format reader that exposes one cue track of a
//! longer file as if it were a file of its own.

use symphonia::core::errors::Result;
use symphonia::core::formats::MediaInfo;
use symphonia::core::formats::{FormatInfo, FormatReader, SeekMode, SeekTo, SeekedTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::Metadata;
use symphonia::core::packet::Packet;
use symphonia::core::units::{Duration, Timestamp};

/// One cue track of an album file, read as if it were a file of its own.
///
/// Wraps the reader of the whole file and passes on only the packets that
/// overlap `[start, end)` of track `track_id`, with timestamps rebased so the
/// track starts at 0. Its track reports the track's own length.
///
/// Packets rarely line up with track boundaries (a FLAC block is ~93 ms, an
/// APE frame over a second), so the first and last packet still carry audio
/// from the neighbouring tracks: decode through [`super::CueTrimDecoder`] to
/// cut them to the exact sample.
pub struct CueTrackReader {
    inner: Box<dyn FormatReader>,
    track_id: u32,
    /// Track start in the file, in the track's timebase units.
    start: u64,
    /// Track end in the file, `None` = end of file.
    end: Option<u64>,
    tracks: Vec<Track>,
    finished: bool,
}

impl CueTrackReader {
    /// Positions `inner` at `start` of track `track_id`.
    pub fn new(mut inner: Box<dyn FormatReader>, track_id: u32, start: u64, end: Option<u64>) -> Result<Self> {
        let mut tracks = inner.tracks().to_vec();
        for track in tracks.iter_mut().filter(|t| t.id == track_id) {
            let file_frames = track.num_frames;
            let len = end.or(file_frames).map(|e| e.saturating_sub(start));
            track.num_frames = len;
            track.start_ts = Timestamp::ZERO;
            track.delay = None;
            track.padding = None;
            track.duration = len.map(Duration::new);
        }
        if start > 0 {
            inner.seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(start.cast_signed()),
                    track_id,
                },
            )?;
        }
        Ok(Self {
            inner,
            track_id,
            start,
            end,
            tracks,
            finished: false,
        })
    }

    /// Length of the track in timebase units, if known.
    pub fn track_len(&self) -> Option<u64> {
        self.tracks.iter().find(|t| t.id == self.track_id).and_then(|t| t.num_frames)
    }

    pub const fn track_id(&self) -> u32 {
        self.track_id
    }
}

impl FormatReader for CueTrackReader {
    fn format_info(&self) -> &FormatInfo {
        self.inner.format_info()
    }

    fn media_info(&self) -> &MediaInfo {
        self.inner.media_info()
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.inner.metadata()
    }

    fn seek(&mut self, mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let rel = match to {
            SeekTo::Timestamp { ts, .. } => ts.get().max(0).unsigned_abs(),
            SeekTo::Time { time, .. } => self
                .tracks
                .iter()
                .find(|t| t.id == self.track_id)
                .and_then(|t| t.time_base)
                .and_then(|tb| tb.calc_timestamp(time))
                .map_or(0, |ts| ts.get().max(0).unsigned_abs()),
        };
        let mut target = self.start + rel;
        if let Some(end) = self.end {
            target = target.min(end.saturating_sub(1));
        }
        let seeked = self.inner.seek(
            mode,
            SeekTo::Timestamp {
                ts: Timestamp::new(target.cast_signed()),
                track_id: self.track_id,
            },
        )?;
        self.finished = false;
        let start = self.start.cast_signed();
        Ok(SeekedTo {
            track_id: seeked.track_id,
            required_ts: Timestamp::new(seeked.required_ts.get() - start),
            actual_ts: Timestamp::new(seeked.actual_ts.get() - start),
        })
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        if self.finished {
            return Ok(None);
        }
        let start = self.start.cast_signed();
        loop {
            let Some(mut packet) = self.inner.next_packet()? else {
                self.finished = true;
                return Ok(None);
            };
            if packet.track_id != self.track_id {
                continue;
            }
            // The decoded (untrimmed) frames cover [first, last).
            let first = packet.pts.get() + packet.trim_start.get().cast_signed();
            let last = first + packet.dur.get().cast_signed();
            if last <= start {
                continue;
            }
            if self.end.is_some_and(|end| first >= end.cast_signed()) {
                self.finished = true;
                return Ok(None);
            }
            packet.pts = Timestamp::new(packet.pts.get() - start);
            packet.dts = Timestamp::new(packet.dts.get() - start);
            return Ok(Some(packet));
        }
    }

    fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
    where
        Self: 's,
    {
        self.inner.into_inner()
    }
}
