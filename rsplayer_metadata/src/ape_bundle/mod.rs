pub mod decoder;
pub mod demuxer;

pub use decoder::ApeDecoder;
pub use demuxer::ApeReader;

use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::common::FourCc;
use symphonia::core::formats::probe::Probe;

pub const CODEC_TYPE_APE: AudioCodecId = AudioCodecId::new(FourCc::new(*b"APE "));

pub fn register_ape_format(probe: &mut Probe) {
    probe.register_format::<ApeReader>();
}

pub fn register_ape_codec(registry: &mut CodecRegistry) {
    registry.register_audio_decoder::<ApeDecoder>();
}
