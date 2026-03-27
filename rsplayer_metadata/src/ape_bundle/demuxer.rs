use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::thread;

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
use ape_decoder::ApeResult;

use super::CODEC_TYPE_APE;

const APE_FORMAT_ID: FormatId = FormatId::new(FourCc::new(*b"APE "));
const APE_FORMAT_INFO: FormatInfo = FormatInfo {
    format: APE_FORMAT_ID,
    short_name: "ape",
    long_name: "Monkey's Audio",
};

type ApeDec = ApeDecoder<Cursor<Vec<u8>>>;
type PrefetchHandle = thread::JoinHandle<ApeResult<Vec<u8>>>;

pub struct ApeReader {
    decoder: Arc<Mutex<ApeDec>>,
    /// Background thread pre-decoding `prefetch_frame`, if any.
    prefetch: Option<PrefetchHandle>,
    /// Frame index the prefetch thread is decoding.
    prefetch_frame: u32,
    tracks: Vec<Track>,
    metadata: MetadataLog,
    total_samples: u64,
    total_frames: u32,
    current_frame: u32,
    sample_rate: u32,
    /// Samples per frame for all non-final frames (`blocks_per_frame`).
    samples_per_frame: u64,
    bytes_per_sample: usize,
    channels: usize,
}

impl ApeReader {
    #[allow(clippy::too_many_lines)]
    pub fn try_new(source: MediaSourceStream<'_>, _options: FormatOptions) -> Result<Self> {
        log::debug!("APE try_new: Starting to read file");
        let mut data = Vec::new();
        let mut reader = source;
        let mut buf = vec![0u8; 65536];
        let mut total_read = 0usize;
        let mut iterations = 0usize;
        loop {
            iterations += 1;
            let n = match reader.read_buf(&mut buf[..]) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    log::warn!("APE try_new: UnexpectedEof after {total_read} bytes, treating as EOF");
                    break;
                }
                Err(e) => {
                    log::error!("APE try_new: read_buf error after {total_read} bytes: {e}");
                    return Err(Error::IoError(e));
                }
            };
            if n == 0 {
                log::debug!("APE try_new: EOF reached after {iterations} iterations, {total_read} bytes");
                break;
            }
            data.extend_from_slice(&buf[..n]);
            total_read += n;
            if iterations.is_multiple_of(100) {
                log::debug!("APE try_new: Read {total_read} bytes so far (iteration {iterations})");
            }
        }

        log::debug!("APE try_new: Read {} bytes from file", data.len());

        let cursor = Cursor::new(data);
        let mut decoder = ApeDecoder::new(cursor).map_err(|e| {
            log::error!("APE try_new: Failed to create decoder: {e}");
            Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        })?;

        let info = decoder.info();
        let total_samples = info.total_samples;
        let total_frames = info.total_frames;
        let sample_rate = info.sample_rate;
        let channels = u32::from(info.channels);
        let bits_per_sample = u32::from(info.bits_per_sample);
        let samples_per_frame = u64::from(info.blocks_per_frame);

        log::info!(
            "APE: {total_frames} frames, {total_samples} total samples, {sample_rate}Hz, {channels} ch, {bits_per_sample} bps, {samples_per_frame} samples/frame",
        );

        let channel_set = match channels {
            1 => Channels::Positioned(Position::FRONT_CENTER),
            3 => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER),
            4 => Channels::Positioned(
                Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::REAR_LEFT | Position::REAR_RIGHT,
            ),
            5 => Channels::Positioned(
                Position::FRONT_LEFT
                    | Position::FRONT_RIGHT
                    | Position::FRONT_CENTER
                    | Position::REAR_LEFT
                    | Position::REAR_RIGHT,
            ),
            6 => Channels::Positioned(
                Position::FRONT_LEFT
                    | Position::FRONT_RIGHT
                    | Position::FRONT_CENTER
                    | Position::LFE1
                    | Position::REAR_LEFT
                    | Position::REAR_RIGHT,
            ),
            _ => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT),
        };

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_TYPE_APE)
            .with_sample_rate(sample_rate)
            .with_channels(channel_set)
            .with_bits_per_sample(bits_per_sample);

        let mut track = Track::new(0);
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_num_frames(total_samples);

        let metadata_log = Self::read_metadata(&mut decoder);

        let bytes_per_sample = (bits_per_sample / 8) as usize;
        let channels = channels as usize;

        let decoder = Arc::new(Mutex::new(decoder));

        // Kick off prefetch for frame 0 in background so decoding overlaps with
        // stream-open and the first ring-buffer fill.
        let prefetch = if total_frames > 0 {
            let dec = Arc::clone(&decoder);
            Some(thread::spawn(move || {
                dec.lock().expect("ape decoder mutex").decode_frame(0)
            }))
        } else {
            None
        };

        Ok(ApeReader {
            decoder,
            prefetch,
            prefetch_frame: 0,
            tracks: vec![track],
            metadata: metadata_log,
            total_samples,
            total_frames,
            current_frame: 0,
            sample_rate,
            samples_per_frame,
            bytes_per_sample,
            channels,
        })
    }

    fn read_metadata(decoder: &mut ApeDec) -> MetadataLog {
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
                    let std_tag = Self::map_ape_field_to_standard_tag(&field.name, value_str);
                    let raw_tag = RawTag::new(field.name.clone(), RawValue::String(Arc::new(value_str.to_string())));
                    let tag = if let Some(std) = std_tag {
                        Tag::new_std(raw_tag, std)
                    } else {
                        Tag::new(raw_tag)
                    };
                    builder.add_tag(tag);
                }
            }
        }

        if let Ok(Some(id3_tag)) = decoder.read_id3v2_tag() {
            for frame in &id3_tag.frames {
                if let Some(value_str) = Self::decode_id3_text_frame(&frame.data) {
                    let std_tag = Self::map_id3_frame_to_standard_tag(&frame.id, &value_str);
                    let raw_tag = RawTag::new(frame.id.clone(), RawValue::String(Arc::new(value_str)));
                    let tag = if let Some(std) = std_tag {
                        Tag::new_std(raw_tag, std)
                    } else {
                        Tag::new(raw_tag)
                    };
                    builder.add_tag(tag);
                }
            }
        }

        metadata_log.push(builder.build());
        metadata_log
    }

    /// Collect the result of a pending prefetch, or decode synchronously.
    ///
    /// Either way the decoder mutex is not held on return.
    fn get_frame_pcm(&mut self, frame_idx: u32) -> Result<Vec<u8>> {
        // If the prefetch thread was working on this exact frame, join it.
        if let Some(handle) = self.prefetch.take() {
            if self.prefetch_frame == frame_idx {
                return handle
                    .join()
                    .map_err(|_| Error::IoError(std::io::Error::other("APE prefetch thread panicked")))?
                    .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())));
            }
            // Prefetch is for a different frame (e.g. after a seek): wait for
            // it to finish so the mutex is free before we lock it below.
            let _ = handle.join();
        }

        // Synchronous fallback.
        self.decoder
            .lock()
            .expect("ape decoder mutex")
            .decode_frame(frame_idx)
            .map_err(|e| Error::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))
    }

    /// Spawn a background thread to decode `frame_idx` so it is ready by the
    /// time the next `next_packet()` call arrives.
    fn spawn_prefetch(&mut self, frame_idx: u32) {
        debug_assert!(self.prefetch.is_none(), "prefetch already in flight");
        let dec = Arc::clone(&self.decoder);
        self.prefetch = Some(thread::spawn(move || {
            dec.lock().expect("ape decoder mutex").decode_frame(frame_idx)
        }));
        self.prefetch_frame = frame_idx;
    }

    fn decode_id3_text_frame(data: &[u8]) -> Option<String> {
        if data.is_empty() {
            return None;
        }
        let encoding = data[0];
        let payload = &data[1..];
        if payload.is_empty() {
            return None;
        }
        let text = match encoding {
            0 => payload.iter().map(|&b| b as char).collect::<String>(),
            1..=3 => String::from_utf8_lossy(payload).into_owned(),
            _ => return None,
        };
        let text = text.trim_end_matches('\0').to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn map_ape_field_to_standard_tag(name: &str, value: &str) -> Option<StandardTag> {
        let name_lower = name.to_ascii_lowercase();
        match name_lower.as_str() {
            "title" => Some(StandardTag::TrackTitle(Arc::new(value.to_string()))),
            "artist" => Some(StandardTag::Artist(Arc::new(value.to_string()))),
            "album" => Some(StandardTag::Album(Arc::new(value.to_string()))),
            "album artist" | "albumartist" => Some(StandardTag::AlbumArtist(Arc::new(value.to_string()))),
            "year" | "date" => Some(StandardTag::RecordingDate(Arc::new(value.to_string()))),
            "track" | "tracknumber" => value.parse().ok().map(StandardTag::TrackNumber),
            "disc" | "discnumber" => value.parse().ok().map(StandardTag::DiscNumber),
            "genre" => Some(StandardTag::Genre(Arc::new(value.to_string()))),
            "comment" => Some(StandardTag::Comment(Arc::new(value.to_string()))),
            "composer" => Some(StandardTag::Composer(Arc::new(value.to_string()))),
            "performer" => Some(StandardTag::Performer(Arc::new(value.to_string()))),
            "label" => Some(StandardTag::Label(Arc::new(value.to_string()))),
            _ => None,
        }
    }

    fn map_id3_frame_to_standard_tag(id: &str, value: &str) -> Option<StandardTag> {
        match id {
            "TIT2" => Some(StandardTag::TrackTitle(Arc::new(value.to_string()))),
            "TPE1" => Some(StandardTag::Artist(Arc::new(value.to_string()))),
            "TALB" => Some(StandardTag::Album(Arc::new(value.to_string()))),
            "TPE2" => Some(StandardTag::AlbumArtist(Arc::new(value.to_string()))),
            "TDRC" | "TYER" | "TDAT" => Some(StandardTag::RecordingDate(Arc::new(value.to_string()))),
            "TRCK" => value
                .split('/')
                .next()
                .and_then(|n| n.parse().ok())
                .map(StandardTag::TrackNumber),
            "TPOS" => value
                .split('/')
                .next()
                .and_then(|n| n.parse().ok())
                .map(StandardTag::DiscNumber),
            "TCON" => Some(StandardTag::Genre(Arc::new(value.to_string()))),
            "COMM" => Some(StandardTag::Comment(Arc::new(value.to_string()))),
            "TCOM" => Some(StandardTag::Composer(Arc::new(value.to_string()))),
            "TPE3" => Some(StandardTag::Performer(Arc::new(value.to_string()))),
            "TPUB" => Some(StandardTag::Label(Arc::new(value.to_string()))),
            _ => None,
        }
    }
}

impl Scoreable for ApeReader {
    fn score(src: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
        let mut src = src;
        let marker = src.read_quad_bytes()?;
        log::debug!("APE score: marker = {:?}, expected = {:?}", marker, b"MAC ");
        if &marker == b"MAC " {
            log::debug!("APE score: MATCH - returning Supported(255)");
            Ok(Score::Supported(255))
        } else {
            log::debug!("APE score: NO MATCH - returning Unsupported");
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
        if self.current_frame >= self.total_frames {
            log::debug!(
                "APE next_packet: EOF reached (frame {}/{})",
                self.current_frame,
                self.total_frames
            );
            return Ok(None);
        }

        // Collect the (possibly pre-decoded) PCM for this frame.
        let pcm = self.get_frame_pcm(self.current_frame)?;

        let frame_start_sample = u64::from(self.current_frame) * self.samples_per_frame;
        let actual_samples = if self.bytes_per_sample > 0 && self.channels > 0 {
            (pcm.len() / (self.bytes_per_sample * self.channels)) as u64
        } else {
            self.samples_per_frame
        };

        log::debug!(
            "APE next_packet: frame {}/{}, pts={}, samples={}",
            self.current_frame,
            self.total_frames,
            frame_start_sample,
            actual_samples,
        );

        let pts = Timestamp::new(frame_start_sample.cast_signed());
        let dur = Duration::new(actual_samples);
        self.current_frame += 1;

        // Kick off background decode of the next frame.  This runs while the
        // caller's ring-buffer write (~1.67 s of real time) drains, so by the
        // time next_packet() is called again the frame is already decoded.
        if self.current_frame < self.total_frames {
            self.spawn_prefetch(self.current_frame);
        }

        Ok(Some(Packet::new(0, pts, dur, pcm)))
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let tb = TimeBase::try_from_recip(self.sample_rate)
            .unwrap_or_else(|| TimeBase::new(NonZero::new(1).unwrap(), NonZero::new(self.sample_rate).unwrap()));

        let required_ts = match to {
            SeekTo::TimeStamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => tb.calc_timestamp(time).unwrap_or(Timestamp::ZERO),
        };

        let sample_index = required_ts.get().unsigned_abs().min(self.total_samples);
        let target_frame = if self.samples_per_frame > 0 {
            #[allow(clippy::cast_possible_truncation)]
            {
                (sample_index / self.samples_per_frame) as u32
            }
        } else {
            0
        };

        self.current_frame = target_frame;
        // Invalidate any in-flight prefetch; get_frame_pcm will join it before
        // acquiring the mutex, so there is no race.
        // (The prefetch handle is intentionally left in self.prefetch so that
        //  get_frame_pcm can detect the frame-index mismatch and join cleanly.)

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
