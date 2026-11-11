//! A cue track as a virtual WAV file, for browser playback.
//!
//! Clients that play songs by URL cannot cut a track out of an album image,
//! so the server decodes just the byte range each request asks for.

use anyhow::{Context, Result, bail};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo};
use symphonia::core::units::Timestamp;

use super::{CueTrackReader, CueTrimDecoder, open_track};

const HEADER_LEN: u64 = 44;

pub struct WavTrack {
    reader: CueTrackReader,
    decoder: CueTrimDecoder,
    channels: u16,
    rate: u32,
    /// 16 for sources up to 16 bits, 24 above.
    bits: u16,
    frames: u64,
}

impl WavTrack {
    /// Opens cue track `key` from `music_dirs`.
    pub fn open(key: &str, music_dirs: &[String]) -> Result<Self> {
        let reader = open_track(key, music_dirs)?;
        let frames = reader.track_len().context("track length unknown")?;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.id == reader.track_id())
            .context("no audio track")?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .context("not an audio track")?
            .clone();
        let rate = params.sample_rate.context("unknown sample rate")?;
        let channels = params
            .channels
            .as_ref()
            .map(symphonia::core::audio::Channels::count)
            .context("unknown channels")?;
        let bits = if params.bits_per_sample.is_some_and(|b| b <= 16) { 16 } else { 24 };
        let inner = crate::build_codec_registry().make_audio_decoder(&params, &AudioDecoderOptions::default())?;
        Ok(Self {
            reader,
            decoder: CueTrimDecoder::new(inner, Some(frames)),
            channels: u16::try_from(channels).context("too many channels")?,
            rate,
            bits,
            frames,
        })
    }

    const fn block_align(&self) -> u64 {
        self.channels as u64 * (self.bits as u64 / 8)
    }

    /// Size of the whole virtual file in bytes.
    pub const fn total_size(&self) -> u64 {
        HEADER_LEN + self.frames * self.block_align()
    }

    fn header(&self) -> Vec<u8> {
        #[allow(clippy::cast_possible_truncation)]
        let data_len = (self.frames * self.block_align()).min(u64::from(u32::MAX) - HEADER_LEN) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let block_align = self.block_align() as u16;
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(data_len + 36).to_le_bytes());
        h.extend_from_slice(b"WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&self.channels.to_le_bytes());
        h.extend_from_slice(&self.rate.to_le_bytes());
        h.extend_from_slice(&(self.rate * u32::from(block_align)).to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&self.bits.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data_len.to_le_bytes());
        h
    }

    /// Bytes `start..=end` of the virtual file.
    pub fn read_range(&mut self, start: u64, end: u64) -> Result<Vec<u8>> {
        let end = end.min(self.total_size().saturating_sub(1));
        if start > end {
            bail!("range starts past the end");
        }
        #[allow(clippy::cast_possible_truncation)]
        let mut out = Vec::with_capacity((end - start + 1) as usize);
        if start < HEADER_LEN {
            let header = self.header();
            #[allow(clippy::cast_possible_truncation)]
            out.extend_from_slice(&header[start as usize..=(end.min(HEADER_LEN - 1)) as usize]);
        }
        if end < HEADER_LEN {
            return Ok(out);
        }
        let data_start = start.max(HEADER_LEN) - HEADER_LEN;
        let data_end = end - HEADER_LEN; // inclusive
        let align = self.block_align();
        let first_frame = data_start / align;
        let skip_bytes = data_start % align;
        let wanted = data_end - data_start + 1;

        if first_frame > 0 {
            self.reader.seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(first_frame.cast_signed()),
                    track_id: self.reader.track_id(),
                },
            )?;
            self.decoder.reset();
        }
        let mut data: Vec<u8> = Vec::new();
        let mut samples: Vec<i32> = Vec::new();
        let sample_bytes = usize::from(self.bits / 8);
        let channels = usize::from(self.channels);
        while (data.len() as u64) < skip_bytes + wanted {
            let Some(packet) = self.reader.next_packet()? else { break };
            // Where the decoder's (head-trimmed) output starts in the track.
            let out_first = (packet.pts.get() + packet.trim_start.get().cast_signed()).max(0).unsigned_abs();
            let buf = self.decoder.decode(&packet)?;
            buf.copy_to_vec_interleaved(&mut samples);
            #[allow(clippy::cast_possible_truncation)]
            let skip_frames = first_frame.saturating_sub(out_first) as usize;
            for frame in samples.chunks_exact(channels).skip(skip_frames) {
                for s in frame {
                    // Full-scale i32: keep the top 2 or 3 bytes.
                    data.extend_from_slice(&s.to_le_bytes()[4 - sample_bytes..]);
                }
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let from = (skip_bytes as usize).min(data.len());
        #[allow(clippy::cast_possible_truncation)]
        let to = (from + wanted as usize).min(data.len());
        out.extend_from_slice(&data[from..to]);
        Ok(out)
    }
}
