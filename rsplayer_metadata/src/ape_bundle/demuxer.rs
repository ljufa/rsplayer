use std::io::{Read, Seek};
use std::sync::{Arc, Mutex};

use symphonia::core::audio::{Channels, Position};
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::common::FourCc;
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::probe::{ProbeDataMatchSpec, ProbeFormatData, ProbeableFormat, Score, Scoreable};
use symphonia::core::formats::{FormatId, FormatInfo, FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo, Track};
use symphonia::core::io::{MediaSourceStream, ReadBytes, ScopedStream};
use symphonia::core::meta::{Metadata, MetadataBuilder, MetadataInfo, MetadataLog, RawTag, RawValue, StandardTag, Tag};
use symphonia::core::packet::Packet;
use symphonia::core::units::{Duration, TimeBase, Timestamp};

use std::num::NonZero;

use ape_decoder::ApeDecoder;

use super::CODEC_TYPE_APE;

const APE_FORMAT_ID: FormatId = FormatId::new(FourCc::new(*b"APE "));
const APE_FORMAT_INFO: FormatInfo = FormatInfo {
    format: APE_FORMAT_ID,
    short_name: "ape",
    long_name: "Monkey's Audio",
};

/// Type-erased APE decoder — allows `ApeReader` to work with any seekable reader.
trait ApeDecoderTrait: Send {
    fn decode_frame(&mut self, frame_idx: u32) -> ape_decoder::ApeResult<Vec<u8>>;
}

impl<R: Read + Seek + Send> ApeDecoderTrait for ApeDecoder<R> {
    fn decode_frame(&mut self, frame_idx: u32) -> ape_decoder::ApeResult<Vec<u8>> {
        ApeDecoder::decode_frame(self, frame_idx)
    }
}

pub struct ApeReader {
    decoder: Mutex<Box<dyn ApeDecoderTrait>>,
    tracks: Vec<Track>,
    metadata: MetadataLog,
    total_samples: u64,
    total_frames: u32,
    current_frame: u32,
    sample_rate: u32,
    samples_per_frame: u64,
    bytes_per_sample: usize,
    channels: usize,
}

impl ApeReader {
    /// Build from a seekable reader (File, `BufReader`<File>, etc.).
    /// Only reads headers + seek table, frame data is read on demand.
    pub fn try_new_from_reader<R: Read + Seek + Send + 'static>(reader: R) -> Result<Self> {
        let start = std::time::Instant::now();
        let mut decoder = ApeDecoder::new(reader).map_err(|e| {
            log::error!("APE: Failed to create decoder: {e}");
            Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        })?;
        log::info!("APE: header parsed in {:?}", start.elapsed());

        let info = decoder.info().clone();
        let metadata_log = Self::read_metadata(&mut decoder);
        Self::build(Box::new(decoder), &info, metadata_log)
    }

    /// Fallback: build from `MediaSourceStream` by reading everything into memory.
    pub fn try_new(source: MediaSourceStream<'_>, _options: FormatOptions) -> Result<Self> {
        let mut data = Vec::new();
        let mut reader = source;
        let mut buf = vec![0u8; 65536];
        loop {
            let n = match reader.read_buf(&mut buf[..]) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(Error::IoError(e)),
            };
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
        log::debug!("APE fallback: read {} bytes into memory", data.len());

        let cursor = std::io::Cursor::new(data);
        let mut decoder = ApeDecoder::new(cursor).map_err(|e| {
            Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        })?;

        let info = decoder.info().clone();
        let metadata_log = Self::read_metadata(&mut decoder);
        Self::build(Box::new(decoder), &info, metadata_log)
    }

    fn build(
        decoder: Box<dyn ApeDecoderTrait>,
        info: &ape_decoder::ApeInfo,
        metadata_log: MetadataLog,
    ) -> Result<Self> {
        let version = info.version;
        let channels = u32::from(info.channels);
        let bits_per_sample = u32::from(info.bits_per_sample);

        log::info!(
            "APE: version={version}, {} frames, {} samples, {}Hz, {channels}ch, {bits_per_sample}bps, compression={}",
            info.total_frames, info.total_samples, info.sample_rate, info.compression_level,
        );

        // ape-decoder only supports version >= 3990 (different entropy coding for 3950-3989).
        if version < 3990 {
            log::error!("APE version {version} not supported (requires >= 3990)");
            return Err(Error::Unsupported("APE version < 3990 not supported"));
        }

        let channel_set = match channels {
            1 => Channels::Positioned(Position::FRONT_CENTER),
            _ => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT),
        };

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_TYPE_APE)
            .with_sample_rate(info.sample_rate)
            .with_channels(channel_set)
            .with_bits_per_sample(bits_per_sample);

        let mut track = Track::new(0);
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_num_frames(info.total_samples);

        Ok(ApeReader {
            decoder: Mutex::new(decoder),
            tracks: vec![track],
            metadata: metadata_log,
            total_samples: info.total_samples,
            total_frames: info.total_frames,
            current_frame: 0,
            sample_rate: info.sample_rate,
            samples_per_frame: u64::from(info.blocks_per_frame),
            bytes_per_sample: (bits_per_sample / 8) as usize,
            channels: channels as usize,
        })
    }

    fn read_metadata<R: Read + Seek>(decoder: &mut ApeDecoder<R>) -> MetadataLog {
        let mut metadata_log = MetadataLog::default();
        let info = MetadataInfo {
            metadata: symphonia::core::meta::METADATA_ID_NULL,
            short_name: "APE",
            long_name: "APEv2 Tag",
        };
        let mut builder = MetadataBuilder::new(info);

        if let Ok(Some(ape_tag)) = decoder.read_tag() {
            for field in &ape_tag.fields {
                if let Some(value_str) = field.value_as_str() {
                    let std_tag = Self::map_ape_tag(&field.name, value_str);
                    let raw_tag = RawTag::new(field.name.clone(), RawValue::String(Arc::new(value_str.to_string())));
                    let tag = match std_tag {
                        Some(std) => Tag::new_std(raw_tag, std),
                        None => Tag::new(raw_tag),
                    };
                    builder.add_tag(tag);
                }
            }
        }

        if let Ok(Some(id3_tag)) = decoder.read_id3v2_tag() {
            for frame in &id3_tag.frames {
                if let Some(value_str) = Self::decode_id3_text(&frame.data) {
                    let std_tag = Self::map_id3_tag(&frame.id, &value_str);
                    let raw_tag = RawTag::new(frame.id.clone(), RawValue::String(Arc::new(value_str)));
                    let tag = match std_tag {
                        Some(std) => Tag::new_std(raw_tag, std),
                        None => Tag::new(raw_tag),
                    };
                    builder.add_tag(tag);
                }
            }
        }

        metadata_log.push(builder.build());
        metadata_log
    }

    fn decode_id3_text(data: &[u8]) -> Option<String> {
        if data.len() < 2 {
            return None;
        }
        let text = match data[0] {
            0 => data[1..].iter().map(|&b| b as char).collect::<String>(),
            1..=3 => String::from_utf8_lossy(&data[1..]).into_owned(),
            _ => return None,
        };
        let text = text.trim_end_matches('\0');
        if text.is_empty() { None } else { Some(text.to_string()) }
    }

    fn map_ape_tag(name: &str, value: &str) -> Option<StandardTag> {
        let v = || Arc::new(value.to_string());
        match name.to_ascii_lowercase().as_str() {
            "title" => Some(StandardTag::TrackTitle(v())),
            "artist" => Some(StandardTag::Artist(v())),
            "album" => Some(StandardTag::Album(v())),
            "album artist" | "albumartist" => Some(StandardTag::AlbumArtist(v())),
            "year" | "date" => Some(StandardTag::RecordingDate(v())),
            "track" | "tracknumber" => value.parse().ok().map(StandardTag::TrackNumber),
            "disc" | "discnumber" => value.parse().ok().map(StandardTag::DiscNumber),
            "genre" => Some(StandardTag::Genre(v())),
            "comment" => Some(StandardTag::Comment(v())),
            "composer" => Some(StandardTag::Composer(v())),
            "performer" => Some(StandardTag::Performer(v())),
            "label" => Some(StandardTag::Label(v())),
            _ => None,
        }
    }

    fn map_id3_tag(id: &str, value: &str) -> Option<StandardTag> {
        let v = || Arc::new(value.to_string());
        match id {
            "TIT2" => Some(StandardTag::TrackTitle(v())),
            "TPE1" => Some(StandardTag::Artist(v())),
            "TALB" => Some(StandardTag::Album(v())),
            "TPE2" => Some(StandardTag::AlbumArtist(v())),
            "TDRC" | "TYER" | "TDAT" => Some(StandardTag::RecordingDate(v())),
            "TRCK" => value.split('/').next().and_then(|n| n.parse().ok()).map(StandardTag::TrackNumber),
            "TPOS" => value.split('/').next().and_then(|n| n.parse().ok()).map(StandardTag::DiscNumber),
            "TCON" => Some(StandardTag::Genre(v())),
            "COMM" => Some(StandardTag::Comment(v())),
            "TCOM" => Some(StandardTag::Composer(v())),
            "TPE3" => Some(StandardTag::Performer(v())),
            "TPUB" => Some(StandardTag::Label(v())),
            _ => None,
        }
    }
}

impl Scoreable for ApeReader {
    fn score(src: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
        let mut src = src;
        let marker = src.read_quad_bytes()?;
        if &marker == b"MAC " {
            Ok(Score::Supported(255))
        } else {
            Ok(Score::Unsupported)
        }
    }
}

impl ProbeableFormat<'_> for ApeReader {
    fn try_probe_new(mss: MediaSourceStream<'_>, opts: FormatOptions) -> Result<Box<dyn FormatReader + '_>> {
        Ok(Box::new(ApeReader::try_new(mss, opts)?))
    }

    fn probe_data() -> &'static [ProbeFormatData] {
        const DATA: &[ProbeFormatData] = &[ProbeFormatData {
            info: APE_FORMAT_INFO,
            spec: ProbeDataMatchSpec {
                extensions: &["ape"],
                mime_types: &["audio/ape", "audio/x-ape", "application/x-ape"],
                markers: &[b"MAC "],
            },
        }];
        DATA
    }
}

impl FormatReader for ApeReader {
    fn format_info(&self) -> &FormatInfo {
        &APE_FORMAT_INFO
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        while self.current_frame < self.total_frames {
            let frame_idx = self.current_frame;
            self.current_frame += 1;
            let value = self.decoder.lock().expect("ape mutex").decode_frame(frame_idx);
            let pcm = match value {
                Ok(pcm) => pcm,
                Err(e) => {
                    log::warn!("APE: skipping frame {frame_idx}/{}: {e}", self.total_frames);
                    continue;
                }
            };

            let frame_start_sample = u64::from(frame_idx) * self.samples_per_frame;
            let actual_samples = if self.bytes_per_sample > 0 && self.channels > 0 {
                (pcm.len() / (self.bytes_per_sample * self.channels)) as u64
            } else {
                self.samples_per_frame
            };

            let pts = Timestamp::new(frame_start_sample.cast_signed());
            let dur = Duration::new(actual_samples);
            return Ok(Some(Packet::new(0, pts, dur, pcm)));
        }

        log::info!("APE: playback complete ({} frames)", self.total_frames);
        Ok(None)
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let tb = TimeBase::try_from_recip(self.sample_rate)
            .unwrap_or_else(|| TimeBase::new(NonZero::new(1).unwrap(), NonZero::new(self.sample_rate).unwrap()));

        let required_ts = match to {
            SeekTo::TimeStamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => tb.calc_timestamp(time).unwrap_or(Timestamp::ZERO),
        };

        let sample_index = required_ts.get().unsigned_abs().min(self.total_samples);
        #[allow(clippy::cast_possible_truncation)]
        let target_frame = if self.samples_per_frame > 0 {
            (sample_index / self.samples_per_frame) as u32
        } else {
            0
        };

        self.current_frame = target_frame;
        let actual_ts = Timestamp::new((u64::from(self.current_frame) * self.samples_per_frame).cast_signed());

        Ok(SeekedTo {
            track_id: 0,
            actual_ts,
            required_ts,
        })
    }

    fn into_inner<'r>(self: Box<Self>) -> MediaSourceStream<'r> {
        unreachable!("ApeReader does not support into_inner")
    }
}
