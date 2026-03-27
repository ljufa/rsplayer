use symphonia::core::audio::{
    AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position,
};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::{CodecInfo, CodecProfileInfo};
use symphonia::core::errors::Result;
use symphonia::core::packet::Packet;

use super::{CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF};

pub struct DsdDecoder {
    params: AudioCodecParameters,
    buf: AudioBuffer<u32>,
}

impl DsdDecoder {
    pub fn try_new(params: &AudioCodecParameters, _options: &AudioDecoderOptions) -> Result<Self> {
        let sample_rate = params.sample_rate.unwrap_or(2_822_400);
        let out_rate = sample_rate / 32;

        let channels = params
            .channels
            .clone()
            .unwrap_or_else(|| Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT));
        let frames_in = params.frames_per_block.unwrap_or(32_768);
        #[allow(clippy::cast_possible_truncation)]
        let capacity = frames_in as usize / 32;

        let spec = AudioSpec::new(out_rate, channels);
        let buf = AudioBuffer::new(spec, capacity);

        Ok(DsdDecoder {
            params: params.clone(),
            buf,
        })
    }

    fn decode_inner(&mut self, packet: &Packet) {
        let channels = self.params.channels.as_ref().map_or(0, Channels::count);
        let src = &packet.data;

        let block_size = src.len() / channels.max(1);
        let samples_out_per_channel = block_size / 4;

        self.buf.clear();
        self.buf.render_uninit(Some(samples_out_per_channel));

        let reverse_bits = self.params.codec == CODEC_TYPE_DSD_LSBF;

        for (c, plane) in self.buf.iter_planes_mut().enumerate() {
            let channel_block = &src[c * block_size..(c + 1) * block_size];
            for (i, chunk) in channel_block.chunks(4).enumerate() {
                if i < plane.len() && chunk.len() == 4 {
                    let mut bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];

                    if reverse_bits {
                        bytes[0] = bytes[0].reverse_bits();
                        bytes[1] = bytes[1].reverse_bits();
                        bytes[2] = bytes[2].reverse_bits();
                        bytes[3] = bytes[3].reverse_bits();
                    }

                    plane[i] = u32::from_le_bytes(bytes);
                }
            }
        }
    }
}

impl AudioDecoder for DsdDecoder {
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

impl RegisterableAudioDecoder for DsdDecoder {
    fn try_registry_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>
    where
        Self: Sized,
    {
        Ok(Box::new(DsdDecoder::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        const CODECS: &[SupportedAudioCodec] = &[
            SupportedAudioCodec {
                id: CODEC_TYPE_DSD_LSBF,
                info: CodecInfo {
                    short_name: "dsd_lsbf",
                    long_name: "DSD (Least Significant Bit First)",
                    profiles: &[] as &[CodecProfileInfo],
                },
            },
            SupportedAudioCodec {
                id: CODEC_TYPE_DSD_MSBF,
                info: CodecInfo {
                    short_name: "dsd_msbf",
                    long_name: "DSD (Most Significant Bit First)",
                    profiles: &[] as &[CodecProfileInfo],
                },
            },
        ];
        CODECS
    }
}
