use symphonia::core::audio::{
    AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position,
};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::{CodecInfo, CodecProfileInfo};
use symphonia::core::errors::Result;
use symphonia::core::packet::Packet;

use super::CODEC_TYPE_APE;

pub struct ApeDecoder {
    params: AudioCodecParameters,
    bits_per_sample: u32,
    buf: AudioBuffer<i32>,
    capacity: usize,
}

impl ApeDecoder {
    pub fn try_new(params: &AudioCodecParameters, _options: &AudioDecoderOptions) -> Result<Self> {
        let sample_rate = params.sample_rate.unwrap_or(44100);
        let bits_per_sample = params.bits_per_sample.unwrap_or(16);

        let channels = params
            .channels
            .clone()
            .unwrap_or_else(|| Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT));

        let spec = AudioSpec::new(sample_rate, channels);
        let capacity = 73728;
        let buf = AudioBuffer::new(spec, capacity);

        Ok(ApeDecoder {
            params: params.clone(),
            bits_per_sample,
            buf,
            capacity,
        })
    }

    fn ensure_capacity(&mut self, samples: usize) {
        if samples > self.capacity {
            let spec = AudioSpec::new(
                self.params.sample_rate.unwrap_or(44100),
                self.params
                    .channels
                    .clone()
                    .unwrap_or_else(|| Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT)),
            );
            self.capacity = samples;
            self.buf = AudioBuffer::new(spec, self.capacity);
        }
    }

    fn decode_inner(&mut self, packet: &Packet) {
        let channels = self.params.channels.as_ref().map_or(2, Channels::count);
        let bits = self.bits_per_sample as usize;
        let bytes_per_sample = bits.div_ceil(8);
        let frame_size = bytes_per_sample * channels;
        let total_bytes = packet.data.len();
        let samples = total_bytes / frame_size;

        log::debug!(
            "APE decode: total_bytes={total_bytes}, channels={channels}, bits={bits}, frame_size={frame_size}, samples={samples}"
        );

        self.ensure_capacity(samples);
        self.buf.clear();
        self.buf.render_uninit(Some(samples));

        for (c, plane) in self.buf.iter_planes_mut().enumerate() {
            match bits {
                8 => {
                    for (i, sample_idx) in (0..total_bytes).step_by(frame_size).enumerate().take(samples) {
                        let offset = sample_idx + c * bytes_per_sample;
                        if offset < total_bytes {
                            plane[i] = (i32::from(packet.data[offset]) - 128) << 24;
                        }
                    }
                }
                16 => {
                    for (i, sample_idx) in (0..total_bytes).step_by(frame_size).enumerate().take(samples) {
                        let offset = sample_idx + c * bytes_per_sample;
                        if offset + 1 < total_bytes {
                            plane[i] =
                                i32::from(i16::from_le_bytes([packet.data[offset], packet.data[offset + 1]])) << 16;
                        }
                    }
                }
                24 => {
                    for (i, sample_idx) in (0..total_bytes).step_by(frame_size).enumerate().take(samples) {
                        let offset = sample_idx + c * bytes_per_sample;
                        if offset + 2 < total_bytes {
                            let val = i32::from(packet.data[offset + 2].cast_signed()) << 16
                                | i32::from(packet.data[offset + 1]) << 8
                                | i32::from(packet.data[offset]);
                            plane[i] = val << 8;
                        }
                    }
                }
                32 => {
                    for (i, sample_idx) in (0..total_bytes).step_by(frame_size).enumerate().take(samples) {
                        let offset = sample_idx + c * bytes_per_sample;
                        if offset + 3 < total_bytes {
                            plane[i] = i32::from_le_bytes([
                                packet.data[offset],
                                packet.data[offset + 1],
                                packet.data[offset + 2],
                                packet.data[offset + 3],
                            ]);
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(plane) = self.buf.plane(0) {
            let first_samples: Vec<i32> = plane.iter().take(10).copied().collect();
            log::debug!("APE decode: first 10 samples of channel 0: {first_samples:?}");
        }
    }
}

impl AudioDecoder for ApeDecoder {
    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs().first().unwrap().info
    }

    fn reset(&mut self) {}

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<GenericAudioBufferRef<'_>> {
        self.decode_inner(packet);
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for ApeDecoder {
    fn try_registry_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>
    where
        Self: Sized,
    {
        Ok(Box::new(ApeDecoder::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        const CODECS: &[SupportedAudioCodec] = &[SupportedAudioCodec {
            id: CODEC_TYPE_APE,
            info: CodecInfo {
                short_name: "ape",
                long_name: "Monkey's Audio (APE)",
                profiles: &[] as &[CodecProfileInfo],
            },
        }];
        CODECS
    }
}
