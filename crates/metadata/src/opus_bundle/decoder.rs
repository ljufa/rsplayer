use symphonia::core::audio::{AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::{CodecInfo, CodecProfileInfo};
use symphonia::core::errors::{Error, Result, decode_error, unsupported_error};
use symphonia::core::packet::PacketRef;

use opus_rs::multistream::{ChannelMappingTable, MultistreamDecoder};

/// Opus always decodes at 48 kHz, whatever rate the source was encoded from.
const OPUS_RATE: u32 = 48_000;

/// Longest Opus packet: 120 ms at 48 kHz (RFC 6716 §3.2.5).
const MAX_FRAMES_PER_PACKET: usize = 5_760;

/// Fields of the Opus identification header the decoder needs.
#[derive(Debug, Clone, PartialEq)]
struct OpusHeader {
    channels: u8,
    /// Encoder delay in 48 kHz samples, to drop from the start of the stream.
    pre_skip: u16,
    /// Output gain, already converted to a linear factor.
    gain: f32,
    mapping_family: u8,
    /// Channel mapping table (`mapping_family != 0`): stream count, coupled
    /// count, then one entry per channel.
    mapping_tail: Vec<u8>,
}

impl OpusHeader {
    /// Parses the identification header Symphonia passes as extra data.
    ///
    /// Ogg and Matroska carry `OpusHead` (RFC 7845 §5.1, version 1,
    /// little-endian). MP4 carries the `dOps` box (version 0, big-endian)
    /// behind the same `OpusHead` magic, so the version byte picks the byte
    /// order.
    fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 19 || &data[..8] != b"OpusHead" {
            return decode_error("opus: invalid identification header");
        }
        let big_endian = data[8] == 0;
        let channels = data[9];
        if channels == 0 {
            return decode_error("opus: zero channels");
        }
        let (pre_skip_bytes, gain_bytes) = ([data[10], data[11]], [data[16], data[17]]);
        let (pre_skip, gain_q8) = if big_endian {
            (u16::from_be_bytes(pre_skip_bytes), i16::from_be_bytes(gain_bytes))
        } else {
            (u16::from_le_bytes(pre_skip_bytes), i16::from_le_bytes(gain_bytes))
        };
        Ok(Self {
            channels,
            pre_skip,
            gain: q8_db_to_linear(gain_q8),
            mapping_family: data[18],
            mapping_tail: data[19..].to_vec(),
        })
    }

    /// Header for a track that came without one (RTP-style mono/stereo).
    const fn for_channels(channels: u8) -> Self {
        Self {
            channels,
            pre_skip: 0,
            gain: 1.0,
            mapping_family: 0,
            mapping_tail: Vec::new(),
        }
    }
}

/// `OpusHead` output gain is Q7.8 dB.
fn q8_db_to_linear(gain_q8: i16) -> f32 {
    10f32.powf(f32::from(gain_q8) / (20.0 * 256.0))
}

/// Symphonia channel layout for an Opus channel count (Vorbis channel order,
/// RFC 7845 §5.1.1.2).
fn channels_for(count: u8) -> Option<Channels> {
    let positions = match count {
        1 => Position::FRONT_LEFT,
        2 => Position::FRONT_LEFT | Position::FRONT_RIGHT,
        3 => Position::FRONT_LEFT | Position::FRONT_CENTER | Position::FRONT_RIGHT,
        4 => Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::REAR_LEFT | Position::REAR_RIGHT,
        5 => Position::FRONT_LEFT | Position::FRONT_CENTER | Position::FRONT_RIGHT | Position::REAR_LEFT | Position::REAR_RIGHT,
        6 => {
            Position::FRONT_LEFT
                | Position::FRONT_CENTER
                | Position::FRONT_RIGHT
                | Position::REAR_LEFT
                | Position::REAR_RIGHT
                | Position::LFE1
        }
        7 => {
            Position::FRONT_LEFT
                | Position::FRONT_CENTER
                | Position::FRONT_RIGHT
                | Position::SIDE_LEFT
                | Position::SIDE_RIGHT
                | Position::REAR_CENTER
                | Position::LFE1
        }
        8 => {
            Position::FRONT_LEFT
                | Position::FRONT_CENTER
                | Position::FRONT_RIGHT
                | Position::SIDE_LEFT
                | Position::SIDE_RIGHT
                | Position::REAR_LEFT
                | Position::REAR_RIGHT
                | Position::LFE1
        }
        _ => return None,
    };
    Some(Channels::Positioned(positions))
}

/// Plane index in Symphonia's buffer (position-bit order) for decoder output
/// channel `ch` (Vorbis order).
const fn plane_for_channel(count: u8, ch: usize) -> usize {
    let map: &[usize] = match count {
        3 => &[0, 2, 1],                // FL, FC, FR
        5 => &[0, 2, 1, 3, 4],          // FL, FC, FR, RL, RR
        6 => &[0, 2, 1, 4, 5, 3],       // FL, FC, FR, RL, RR, LFE
        7 => &[0, 2, 1, 5, 6, 4, 3],    // FL, FC, FR, SL, SR, RC, LFE
        8 => &[0, 2, 1, 6, 7, 4, 5, 3], // FL, FC, FR, SL, SR, RL, RR, LFE
        _ => return ch,
    };
    map[ch]
}

enum Inner {
    /// Mapping family 0: one mono or stereo stream.
    Single(Box<opus_rs::OpusDecoder>),
    /// Mapping family 1: several streams routed by the mapping table.
    Multi(Box<MultistreamDecoder>),
}

impl Inner {
    fn new(header: &OpusHeader) -> Result<Self> {
        match header.mapping_family {
            0 if header.channels <= 2 => {
                let dec = opus_rs::OpusDecoder::new(OPUS_RATE.cast_signed(), usize::from(header.channels)).map_err(Error::DecodeError)?;
                Ok(Self::Single(Box::new(dec)))
            }
            0 => decode_error("opus: mapping family 0 allows at most 2 channels"),
            1 => {
                let Some(table) = ChannelMappingTable::parse(1, header.channels, &header.mapping_tail) else {
                    return decode_error("opus: invalid channel mapping table");
                };
                let dec = MultistreamDecoder::new(OPUS_RATE.cast_signed(), table).map_err(Error::DecodeError)?;
                Ok(Self::Multi(Box::new(dec)))
            }
            _ => unsupported_error("opus: unsupported channel mapping family"),
        }
    }

    fn decode(&mut self, data: &[u8], out: &mut [f32]) -> Result<usize> {
        match self {
            Self::Single(dec) => dec.decode(data, MAX_FRAMES_PER_PACKET, out),
            Self::Multi(dec) => dec.decode(data, MAX_FRAMES_PER_PACKET, out),
        }
        .map_err(Error::DecodeError)
    }
}

pub struct OpusDecoder {
    params: AudioCodecParameters,
    opts: AudioDecoderOptions,
    header: OpusHeader,
    inner: Inner,
    /// Pre-skip frames still to drop (see [`Self::start_trim`]).
    pending_skip: usize,
    /// Interleaved decoder output, `MAX_FRAMES_PER_PACKET * channels`.
    pcm: Vec<f32>,
    buf: AudioBuffer<f32>,
}

impl OpusDecoder {
    pub fn try_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Self> {
        if params.codec != CODEC_ID_OPUS {
            return unsupported_error("opus: invalid codec");
        }

        let header = if let Some(data) = params.extra_data.as_deref() {
            OpusHeader::parse(data)?
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let count = params.channels.as_ref().map_or(2, Channels::count) as u8;
            OpusHeader::for_channels(count)
        };

        let Some(channels) = channels_for(header.channels) else {
            return unsupported_error("opus: unsupported channel count");
        };

        let inner = Inner::new(&header)?;
        let channel_count = usize::from(header.channels);

        Ok(Self {
            params: params.clone(),
            opts: *opts,
            header,
            inner,
            pending_skip: 0,
            pcm: vec![0.0; MAX_FRAMES_PER_PACKET * channel_count],
            buf: AudioBuffer::new(AudioSpec::new(OPUS_RATE, channels), MAX_FRAMES_PER_PACKET),
        })
    }

    fn decode_inner(&mut self, packet: &PacketRef<'_>) -> Result<()> {
        self.buf.clear();

        let frames = self.inner.decode(packet.data, &mut self.pcm)?;
        if frames > MAX_FRAMES_PER_PACKET {
            return decode_error("opus: packet longer than 120 ms");
        }
        self.buf.render_uninit(Some(frames));

        let count = self.header.channels;
        let stride = usize::from(count);
        let gain = self.header.gain;
        for ch in 0..stride {
            let Some(plane) = self.buf.plane_mut(plane_for_channel(count, ch)) else {
                return decode_error("opus: missing audio plane");
            };
            for (dst, frame) in plane[..frames].iter_mut().zip(self.pcm.chunks_exact(stride)) {
                *dst = frame[ch] * gain;
            }
        }

        if self.opts.gapless {
            let start = self.start_trim(packet, frames);
            #[allow(clippy::cast_possible_truncation)]
            self.buf.trim(start, packet.trim_end.get() as usize);
        }
        Ok(())
    }

    /// Frames to drop from the start of this packet.
    ///
    /// Symphonia's demuxers expose the Opus pre-skip only as `Track::delay`
    /// and never mark it on the packets, so the decoder applies it: from the
    /// first packet of the stream (pts 0 in Ogg/MP4, negative in Matroska,
    /// which shifts time back by the codec delay) until `pre_skip` frames are
    /// gone, possibly across several short packets. A container that does mark a
    /// start trim is trusted instead, so the skip is never applied twice.
    fn start_trim(&mut self, packet: &PacketRef<'_>, frames: usize) -> usize {
        #[allow(clippy::cast_possible_truncation)]
        let container_trim = packet.trim_start.get() as usize;
        if container_trim > 0 {
            self.pending_skip = 0;
            return container_trim;
        }
        if packet.pts.get() <= 0 {
            self.pending_skip = usize::from(self.header.pre_skip);
        }
        let skip = self.pending_skip.min(frames);
        self.pending_skip -= skip;
        skip
    }
}

impl AudioDecoder for OpusDecoder {
    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()[0].info
    }

    fn reset(&mut self) {
        // opus-rs has no reset; a fresh decoder drops the inter-frame state
        // (overlap, pitch history) left over from before a seek.
        if let Ok(inner) = Inner::new(&self.header) {
            self.inner = inner;
        }
        self.pending_skip = 0;
        self.buf.clear();
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        if let Err(e) = self.decode_inner(packet) {
            self.buf.clear();
            return Err(e);
        }
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>
    where
        Self: Sized,
    {
        Ok(Box::new(Self::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        const CODECS: &[SupportedAudioCodec] = &[SupportedAudioCodec {
            id: CODEC_ID_OPUS,
            info: CodecInfo {
                short_name: "opus",
                long_name: "Opus",
                profiles: &[] as &[CodecProfileInfo],
            },
        }];
        CODECS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::Audio;
    use symphonia::core::units::{Duration, Timestamp};

    fn opus_head(channels: u8, gain_q8: i16, family: u8, tail: &[u8]) -> Vec<u8> {
        opus_head_skip(channels, 312, gain_q8, family, tail)
    }

    fn opus_head_skip(channels: u8, pre_skip: u16, gain_q8: i16, family: u8, tail: &[u8]) -> Vec<u8> {
        let mut h = b"OpusHead".to_vec();
        h.push(1);
        h.push(channels);
        h.extend_from_slice(&pre_skip.to_le_bytes());
        h.extend_from_slice(&44_100u32.to_le_bytes());
        h.extend_from_slice(&gain_q8.to_le_bytes());
        h.push(family);
        h.extend_from_slice(tail);
        h
    }

    fn params(extra: Vec<u8>) -> AudioCodecParameters {
        let mut p = AudioCodecParameters::new();
        p.for_codec(CODEC_ID_OPUS)
            .with_sample_rate(OPUS_RATE)
            .with_extra_data(extra.into_boxed_slice());
        p
    }

    /// 20 ms stereo packets of a 440 Hz tone, encoded with opus-rs.
    fn encode_tone(packets: usize) -> Vec<Vec<u8>> {
        let mut enc = opus_rs::OpusEncoder::new(48_000, 2, opus_rs::Application::Audio).unwrap();
        let frame = 960;
        let mut out = Vec::new();
        for p in 0..packets {
            let pcm: Vec<f32> = (0..frame)
                .flat_map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let t = (p * frame + i) as f32 / 48_000.0;
                    let s = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                    [s, s]
                })
                .collect();
            let mut data = vec![0u8; 1276];
            let n = enc.encode(&pcm, frame, &mut data).unwrap();
            data.truncate(n);
            out.push(data);
        }
        out
    }

    /// The `i`-th 20 ms packet of a stream.
    fn packet_at(i: usize, data: &[u8]) -> PacketRef<'_> {
        PacketRef::new(0, Timestamp::new(i64::try_from(i * 960).unwrap()), Duration::new(960), data)
    }

    fn peak(buf: &GenericAudioBufferRef<'_>) -> f32 {
        match buf {
            GenericAudioBufferRef::F32(b) => b.iter_planes().flat_map(|p| p.iter()).fold(0.0f32, |m, s| m.max(s.abs())),
            _ => panic!("expected f32 output"),
        }
    }

    #[test]
    fn parses_ogg_and_mp4_headers() {
        let ogg = OpusHeader::parse(&opus_head(2, 256 * 6, 0, &[])).unwrap();
        assert_eq!(ogg.channels, 2);
        assert_eq!(ogg.pre_skip, 312);
        assert!((ogg.gain - 1.995).abs() < 0.01, "{}", ogg.gain);

        // dOps: version 0, big-endian fields.
        let mut mp4 = b"OpusHead".to_vec();
        mp4.extend_from_slice(&[0, 2, 0x01, 0x38, 0, 0, 0xAC, 0x44]);
        mp4.extend_from_slice(&(-256i16 * 6).to_be_bytes());
        mp4.push(0);
        let mp4 = OpusHeader::parse(&mp4).unwrap();
        assert_eq!(mp4.pre_skip, 312);
        assert!((mp4.gain - 0.501).abs() < 0.01, "{}", mp4.gain);

        assert!(OpusHeader::parse(b"NotOpus!").is_err());
    }

    #[test]
    fn decodes_stereo_tone() {
        let mut dec = OpusDecoder::try_new(&params(opus_head(2, 0, 0, &[])), &AudioDecoderOptions::default()).unwrap();
        let mut total = 0;
        let mut loudest = 0.0f32;
        for (i, data) in encode_tone(25).iter().enumerate() {
            let pkt = packet_at(i, data);
            let out = dec.decode_ref(&pkt).unwrap();
            assert_eq!(out.spec().rate(), 48_000);
            assert_eq!(out.spec().channels().count(), 2);
            total += out.frames();
            loudest = loudest.max(peak(&out));
        }
        assert_eq!(total, 25 * 960 - 312, "pre-skip dropped");
        assert!(loudest > 0.3 && loudest < 0.7, "peak {loudest}");
    }

    #[test]
    fn applies_header_gain_and_trims() {
        let packets = encode_tone(10);
        let opts = AudioDecoderOptions::default();
        let mut plain = OpusDecoder::try_new(&params(opus_head(2, 0, 0, &[])), &opts).unwrap();
        // -6.02 dB halves the amplitude.
        let mut quiet = OpusDecoder::try_new(&params(opus_head(2, -1541, 0, &[])), &opts).unwrap();

        let (mut p_plain, mut p_quiet) = (0.0f32, 0.0f32);
        for (i, data) in packets.iter().enumerate() {
            let mut pkt = packet_at(i, data);
            p_plain = p_plain.max(peak(&plain.decode_ref(&pkt).unwrap()));
            if i == 0 {
                pkt.trim_start = Duration::new(312);
            }
            let out = quiet.decode_ref(&pkt).unwrap();
            if i == 0 {
                assert_eq!(out.frames(), 960 - 312);
            }
            p_quiet = p_quiet.max(peak(&out));
        }
        assert!((p_quiet / p_plain - 0.5).abs() < 0.02, "{p_plain} {p_quiet}");
    }

    #[test]
    fn pre_skip_spans_packets_and_rearms_at_start() {
        let packets = encode_tone(3);
        let mut dec = OpusDecoder::try_new(&params(opus_head_skip(2, 1500, 0, 0, &[])), &AudioDecoderOptions::default()).unwrap();
        let frames = |dec: &mut OpusDecoder, i: usize| {
            let pkt = packet_at(i, &packets[i]);
            dec.decode_ref(&pkt).unwrap().frames()
        };
        assert_eq!(frames(&mut dec, 0), 0);
        assert_eq!(frames(&mut dec, 1), 960 * 2 - 1500);
        assert_eq!(frames(&mut dec, 2), 960);

        // A seek mid-stream drops nothing; back to the start skips again.
        dec.reset();
        assert_eq!(frames(&mut dec, 2), 960);
        dec.reset();
        assert_eq!(frames(&mut dec, 0), 0);
    }

    #[test]
    fn surround_family_one_is_accepted() {
        // 5.1: 4 streams, 2 coupled, Vorbis order mapping.
        let head = opus_head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5]);
        let dec = OpusDecoder::try_new(&params(head), &AudioDecoderOptions::default()).unwrap();
        assert_eq!(dec.buf.spec().channels().count(), 6);
        assert_eq!(plane_for_channel(6, 5), 3, "LFE goes to the LFE plane");
    }

    #[test]
    fn rejects_reserved_mapping_family() {
        let head = opus_head(2, 0, 255, &[1, 1, 0, 1]);
        assert!(OpusDecoder::try_new(&params(head), &AudioDecoderOptions::default()).is_err());
    }
}
