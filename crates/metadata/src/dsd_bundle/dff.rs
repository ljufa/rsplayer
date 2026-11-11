//! DSDIFF (`.dff`) demuxer.
//!
//! A DSDIFF file is a big-endian `FRM8` form of type `DSD ` holding chunks:
//! `PROP` (sample rate, channels, compression), `DSD ` (the audio, one byte
//! per channel in turn, most significant bit first) and optionally `ID3 `
//! (an `ID3v2` tag, sometimes appended after the form by taggers).
//!
//! Packets are de-interleaved to the planar layout [`super::DsdDecoder`]
//! expects (all bytes of channel 0, then channel 1, ...), as for SACD ISO
//! images. DST-compressed files are rejected: they need a DST decoder.

use std::io::{Seek, SeekFrom};
use std::num::NonZero;

use symphonia::core::audio::{Channels, Position};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::common::FourCc;
use symphonia::core::errors::{Result, decode_error, unsupported_error};
use symphonia::core::formats::probe::{ProbeDataMatchSpec, ProbeFormatData, ProbeableFormat, Score, Scoreable};
use symphonia::core::formats::{FormatId, FormatInfo, FormatOptions, FormatReader, MediaInfo, SeekMode, SeekTo, SeekedTo, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream, ReadBytes, ScopedStream};
use symphonia::core::meta::{Metadata, MetadataBuilder, MetadataLog, MetadataSideData};
use symphonia::core::packet::Packet;
use symphonia::core::units::{Duration, TimeBase, Timestamp};

use symphonia_metadata::id3v2::{ID3V2_METADATA_INFO, read_id3v2};

use super::CODEC_TYPE_DSD_MSBF;

const DFF_FORMAT_ID: FormatId = FormatId::new(FourCc::new(*b"DFF "));
const DFF_FORMAT_INFO: FormatInfo = FormatInfo {
    format: DFF_FORMAT_ID,
    short_name: "dff",
    long_name: "DSDIFF (Direct Stream Digital Interchange File Format)",
};

/// Bytes per channel in one packet: 32768 DSD samples, as a DSF block.
const BYTES_PER_CHANNEL_PER_PACKET: u64 = 4096;
/// Error text for DST-compressed files, recognized by the scan summary.
pub const DST_UNSUPPORTED: &str = "dff: DST-compressed DSDIFF is not supported";
/// DSD64, the rate assumed when the `FS  ` chunk is missing.
const DEFAULT_SAMPLE_RATE: u32 = 2_822_400;

/// What the chunks before (and after) the audio say about a DSDIFF file.
#[derive(Debug, Default, PartialEq, Eq)]
struct DffInfo {
    sample_rate: Option<u32>,
    channel_count: u16,
    /// Offset and length of the `DSD ` chunk's audio data.
    data: Option<(u64, u64)>,
    /// Offset of the `ID3 ` chunk's data.
    id3: Option<u64>,
}

impl DffInfo {
    fn read(source: &mut MediaSourceStream) -> Result<Self> {
        if source.read_quad_bytes()? != *b"FRM8" {
            return unsupported_error("dff: missing FRM8 marker");
        }
        let form_size = source.read_be_u64()?;
        if source.read_quad_bytes()? != *b"DSD " {
            return unsupported_error("dff: not a DSD form");
        }
        let form_end = 12u64.saturating_add(form_size);
        let seekable = source.is_seekable();
        let mut info = Self::default();

        while source.pos() + 12 <= form_end {
            let Ok(id) = source.read_quad_bytes() else { break };
            let size = source.read_be_u64()?;
            let start = source.pos();
            match &id {
                b"PROP" => info.read_prop(source, start + size)?,
                b"DSD " => {
                    // A writer that never finished may leave the size 0 or
                    // too large: the data then runs to the end of the file.
                    let len = match source.byte_len() {
                        Some(total) if size == 0 || start + size > total => total.saturating_sub(start),
                        _ => size,
                    };
                    info.data = Some((start, len));
                    if !seekable {
                        break;
                    }
                }
                b"DST " => return unsupported_error(DST_UNSUPPORTED),
                b"ID3 " => info.id3 = Some(start),
                _ => {}
            }
            let next = start.saturating_add(size).saturating_add(size & 1);
            if !Self::skip_to(source, next, seekable) {
                break;
            }
        }

        // Some taggers append the ID3 chunk after the form without growing it.
        if info.id3.is_none()
            && seekable
            && source.seek(SeekFrom::Start(form_end)).is_ok()
            && source.read_quad_bytes().is_ok_and(|id| id == *b"ID3 ")
            && source.read_be_u64().is_ok()
        {
            info.id3 = Some(source.pos());
        }

        if info.channel_count == 0 {
            return decode_error("dff: missing channel count");
        }
        if info.data.is_none() {
            return decode_error("dff: missing DSD sound data chunk");
        }
        Ok(info)
    }

    /// Reads the `PROP` chunk's sub-chunks up to `end`.
    fn read_prop(&mut self, source: &mut MediaSourceStream, end: u64) -> Result<()> {
        if source.read_quad_bytes()? != *b"SND " {
            return Ok(());
        }
        while source.pos() + 12 <= end {
            let id = source.read_quad_bytes()?;
            let size = source.read_be_u64()?;
            let start = source.pos();
            match &id {
                b"FS  " => self.sample_rate = Some(source.read_be_u32()?),
                b"CHNL" => self.channel_count = source.read_be_u16()?,
                b"CMPR" => {
                    let compression = source.read_quad_bytes()?;
                    if compression != *b"DSD " {
                        return unsupported_error(DST_UNSUPPORTED);
                    }
                }
                _ => {}
            }
            let next = start.saturating_add(size).saturating_add(size & 1);
            source.ignore_bytes(next.saturating_sub(source.pos()))?;
        }
        Ok(())
    }

    /// Moves to `pos`, by seeking when possible; `false` when it cannot.
    fn skip_to(source: &mut MediaSourceStream, pos: u64, seekable: bool) -> bool {
        let here = source.pos();
        if pos < here {
            return false;
        }
        if seekable {
            source.seek(SeekFrom::Start(pos)).is_ok_and(|p| p == pos)
        } else {
            source.ignore_bytes(pos - here).is_ok()
        }
    }
}

fn channel_layout(count: u16) -> Channels {
    let front = Position::FRONT_LEFT | Position::FRONT_RIGHT;
    match count {
        1 => Channels::Positioned(Position::FRONT_CENTER),
        2 => Channels::Positioned(front),
        5 => Channels::Positioned(front | Position::FRONT_CENTER | Position::REAR_LEFT | Position::REAR_RIGHT),
        6 => Channels::Positioned(
            front | Position::FRONT_CENTER | Position::LFE1 | Position::REAR_LEFT | Position::REAR_RIGHT,
        ),
        n => Channels::Discrete(n),
    }
}

pub struct DffReader<'s> {
    reader: MediaSourceStream<'s>,
    tracks: Vec<Track>,
    metadata: MetadataLog,
    media_info: MediaInfo,
    sample_rate: u32,
    channels: usize,
    data_start: u64,
    data_len: u64,
    /// Bytes of audio data read so far (all channels).
    position: u64,
}

impl<'s> DffReader<'s> {
    /// Reads the file's properties and tags and positions at the audio.
    ///
    /// # Errors
    /// When the file is not DSDIFF, is DST-compressed, or lacks the channel
    /// count or the sound data chunk.
    pub fn try_new(mut source: MediaSourceStream<'s>, _options: FormatOptions) -> Result<Self> {
        let info = DffInfo::read(&mut source)?;
        let (data_start, data_len) = info.data.unwrap_or_default();
        let channels = usize::from(info.channel_count);
        let sample_rate = info.sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE);
        // Whole frames only: a byte for every channel.
        let data_len = data_len - data_len % channels as u64;

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_TYPE_DSD_MSBF)
            .with_sample_rate(sample_rate)
            .with_channels(channel_layout(info.channel_count))
            .with_frames_per_block(BYTES_PER_CHANNEL_PER_PACKET * 8);

        let mut track = Track::new(0);
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_num_frames(data_len / channels as u64 * 8);

        let mut metadata = MetadataLog::default();
        if let Some(id3) = info.id3
            && source.seek(SeekFrom::Start(id3)).is_ok_and(|p| p == id3)
        {
            let mut builder = MetadataBuilder::new(ID3V2_METADATA_INFO);
            let mut side_data: Vec<MetadataSideData> = Vec::new();
            match read_id3v2(&mut source, &mut builder, &mut side_data) {
                Ok(()) => metadata.push(builder.build()),
                Err(e) => log::warn!("Failed to read ID3v2 metadata from DFF: {e}"),
            }
        }
        if source.pos() != data_start {
            source.seek(SeekFrom::Start(data_start))?;
        }

        let media_info = MediaInfo::from_track(&track);
        Ok(Self {
            reader: source,
            tracks: vec![track],
            metadata,
            media_info,
            sample_rate,
            channels,
            data_start,
            data_len,
            position: 0,
        })
    }

    const fn bytes_per_packet(&self) -> u64 {
        BYTES_PER_CHANNEL_PER_PACKET * self.channels as u64
    }
}

/// `[c0, c1, c0, c1, ...]` → `[c0, c0, ..., c1, c1, ...]`.
fn deinterleave(interleaved: &[u8], channels: usize) -> Box<[u8]> {
    if channels <= 1 {
        return interleaved.into();
    }
    let per_channel = interleaved.len() / channels;
    let mut planar = vec![0u8; per_channel * channels];
    for (frame, bytes) in interleaved.chunks_exact(channels).enumerate() {
        for (ch, &byte) in bytes.iter().enumerate() {
            planar[ch * per_channel + frame] = byte;
        }
    }
    planar.into_boxed_slice()
}

impl Scoreable for DffReader<'_> {
    fn score(mut src: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
        if src.read_quad_bytes()? != *b"FRM8" {
            return Ok(Score::Unsupported);
        }
        let _size = src.read_be_u64()?;
        if src.read_quad_bytes()? == *b"DSD " {
            Ok(Score::Supported(255))
        } else {
            Ok(Score::Unsupported)
        }
    }
}

impl ProbeableFormat<'_> for DffReader<'_> {
    fn try_probe_new(mss: MediaSourceStream<'_>, opts: FormatOptions) -> Result<Box<dyn FormatReader + '_>> {
        Ok(Box::new(DffReader::try_new(mss, opts)?))
    }

    fn probe_data() -> &'static [ProbeFormatData] {
        const DATA: &[ProbeFormatData] = &[ProbeFormatData {
            info: DFF_FORMAT_INFO,
            spec: ProbeDataMatchSpec {
                extensions: &["dff"],
                mime_types: &["audio/x-dff"],
                markers: &[b"FRM8"],
            },
        }];
        DATA
    }
}

impl FormatReader for DffReader<'_> {
    fn format_info(&self) -> &FormatInfo {
        &DFF_FORMAT_INFO
    }

    fn media_info(&self) -> &MediaInfo {
        &self.media_info
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        let left = self.data_len - self.position;
        if left == 0 {
            return Ok(None);
        }
        #[allow(clippy::cast_possible_truncation)]
        let len = left.min(self.bytes_per_packet()) as usize;
        let buf = self.reader.read_boxed_slice_exact(len)?;
        let channels = self.channels as u64;
        let pts = Timestamp::new((self.position / channels * 8).cast_signed());
        let dur = Duration::new(len as u64 / channels * 8);
        self.position += len as u64;
        Ok(Some(Packet::new(0, pts, dur, deinterleave(&buf, self.channels))))
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let tb = TimeBase::try_from_recip(self.sample_rate).unwrap_or_else(|| {
            TimeBase::new(
                NonZero::new(1).expect("nonzero"),
                NonZero::new(self.sample_rate.max(1)).expect("nonzero"),
            )
        });
        let required_ts = match to {
            SeekTo::Timestamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => tb.calc_timestamp(time).unwrap_or(Timestamp::ZERO),
        };

        // Packet-aligned, like the DSF reader.
        let frames_per_packet = BYTES_PER_CHANNEL_PER_PACKET * 8;
        let total_frames = self.data_len / self.channels as u64 * 8;
        let target = required_ts.get().max(0).unsigned_abs().min(total_frames);
        let packet = target / frames_per_packet;
        let offset = (packet * self.bytes_per_packet()).min(self.data_len);

        self.reader.seek(SeekFrom::Start(self.data_start + offset))?;
        self.position = offset;

        Ok(SeekedTo {
            track_id: 0,
            actual_ts: Timestamp::new((offset / self.channels as u64 * 8).cast_signed()),
            required_ts,
        })
    }

    fn into_inner<'r>(self: Box<Self>) -> MediaSourceStream<'r>
    where
        Self: 'r,
    {
        self.reader
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, clippy::trivially_copy_pass_by_ref)]
mod tests {
    use std::io::Cursor;

    use symphonia::core::io::MediaSourceStreamOptions;

    use super::*;

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&(data.len() as u64).to_be_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    /// A stereo DSD64 file with `frames` bytes per channel: left bytes are
    /// `0xA0 + i`, right bytes `0x50 + i` (mod 256).
    fn dff(frames: usize, compression: &[u8; 4], id3_after_form: Option<&[u8]>) -> Vec<u8> {
        let mut prop = b"SND ".to_vec();
        prop.extend(chunk(b"FS  ", &2_822_400u32.to_be_bytes()));
        let mut chnl = 2u16.to_be_bytes().to_vec();
        chnl.extend_from_slice(b"SLFTSRGT");
        prop.extend(chunk(b"CHNL", &chnl));
        let mut cmpr = compression.to_vec();
        cmpr.extend_from_slice(b"\x0enot compressed\0");
        prop.extend(chunk(b"CMPR", &cmpr));

        let mut audio = Vec::new();
        for i in 0..frames {
            audio.push(0xA0u8.wrapping_add(i as u8));
            audio.push(0x50u8.wrapping_add(i as u8));
        }
        let mut body = b"DSD ".to_vec();
        body.extend(chunk(b"FVER", &0x0105_0000u32.to_be_bytes()));
        body.extend(chunk(b"PROP", &prop));
        body.extend(chunk(b"DSD ", &audio));
        let mut file = chunk(b"FRM8", &body);
        if let Some(tag) = id3_after_form {
            file.extend(chunk(b"ID3 ", tag));
        }
        file
    }

    fn reader(bytes: Vec<u8>) -> Result<DffReader<'static>> {
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), MediaSourceStreamOptions::default());
        DffReader::try_new(mss, FormatOptions::default())
    }

    /// Minimal ID3v2.3 tag with one TIT2 frame.
    fn id3_title(title: &str) -> Vec<u8> {
        let mut frame = b"TIT2".to_vec();
        frame.extend_from_slice(&(title.len() as u32 + 1).to_be_bytes());
        frame.extend_from_slice(&[0, 0, 0]);
        frame.extend_from_slice(title.as_bytes());
        let size = frame.len() as u32;
        let syncsafe = [(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f];
        let mut tag = b"ID3\x03\x00\x00".to_vec();
        tag.extend_from_slice(&syncsafe);
        tag.extend(frame);
        tag
    }

    #[test]
    fn reads_properties_and_planar_packets() {
        let frames = 5000;
        let mut r = reader(dff(frames, b"DSD ", None)).unwrap();
        let track = &r.tracks()[0];
        assert_eq!(track.num_frames, Some(frames as u64 * 8));
        let Some(CodecParameters::Audio(p)) = &track.codec_params else { panic!() };
        assert_eq!(p.sample_rate, Some(2_822_400));
        assert_eq!(p.codec, CODEC_TYPE_DSD_MSBF);
        assert_eq!(p.channels.as_ref().map(Channels::count), Some(2));

        let first = r.next_packet().unwrap().unwrap();
        assert_eq!(first.data.len(), 2 * 4096);
        assert_eq!(&first.data[..2], &[0xA0, 0xA1]);
        assert_eq!(&first.data[4096..4098], &[0x50, 0x51]);
        let second = r.next_packet().unwrap().unwrap();
        assert_eq!(second.pts, Timestamp::new(4096 * 8));
        assert_eq!(second.data.len(), 2 * (frames - 4096));
        assert!(r.next_packet().unwrap().is_none());
    }

    #[test]
    fn seeks_to_packet_boundary() {
        let mut r = reader(dff(10_000, b"DSD ", None)).unwrap();
        let seeked = r
            .seek(SeekMode::Accurate, SeekTo::Timestamp { ts: Timestamp::new(5000 * 8), track_id: 0 })
            .unwrap();
        assert_eq!(seeked.actual_ts, Timestamp::new(4096 * 8));
        let p = r.next_packet().unwrap().unwrap();
        assert_eq!(p.data[0], 0xA0u8.wrapping_add(4096u32 as u8));
    }

    #[test]
    fn rejects_dst() {
        let err = reader(dff(100, b"DST ", None)).err().expect("DST must fail");
        assert!(err.to_string().contains("DST"), "{err}");
    }

    #[test]
    fn reads_id3_appended_after_the_form() {
        let mut r = reader(dff(100, b"DSD ", Some(&id3_title("Hello")))).unwrap();
        let rev = r.metadata().skip_to_latest().cloned().expect("metadata");
        let title = rev.media.tags.iter().find(|t| t.raw.key == "TIT2").expect("title");
        assert_eq!(title.raw.value.to_string(), "Hello");
        // Audio still starts at the data chunk.
        assert_eq!(r.next_packet().unwrap().unwrap().data[0], 0xA0);
    }

    #[test]
    fn library_probe_picks_the_dff_reader() {
        use symphonia::core::formats::probe::Hint;
        use symphonia::core::meta::MetadataOptions;
        let mss = MediaSourceStream::new(
            // One second of DSD64: 2_822_400 / 8 bytes per channel.
            Box::new(Cursor::new(dff(352_800, b"DSD ", Some(&id3_title("Song"))))),
            MediaSourceStreamOptions::default(),
        );
        let mut hint = Hint::new();
        hint.with_extension("dff");
        let mut format = crate::build_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .unwrap();
        assert_eq!(format.format_info().short_name, "dff");
        let (song, _) = crate::audio_metadata_extractor::AudioMetadataExtractor::extract(&mut *format);
        assert_eq!(song.title.as_deref(), Some("Song"));
        assert_eq!(song.time, Some(std::time::Duration::from_secs(1)));
    }

    #[test]
    fn deinterleaves_channels() {
        assert_eq!(&*deinterleave(&[1, 2, 3, 4, 5, 6], 3), &[1, 4, 2, 5, 3, 6]);
        assert_eq!(&*deinterleave(&[1, 2, 3], 1), &[1, 2, 3]);
    }
}
