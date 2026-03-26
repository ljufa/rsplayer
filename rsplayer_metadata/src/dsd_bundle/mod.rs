pub mod decoder;
pub mod demuxer;
pub mod dsf;

pub use decoder::DsdDecoder;
pub use demuxer::DsfReader;

use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::common::FourCc;
use symphonia::core::formats::probe::Probe;

/// DSD codec ID constants — defined in the user namespace via `FourCC`
/// since upstream symphonia-core does not include them.
pub const CODEC_TYPE_DSD_LSBF: AudioCodecId = AudioCodecId::new(FourCc::new(*b"DsdL"));
pub const CODEC_TYPE_DSD_MSBF: AudioCodecId = AudioCodecId::new(FourCc::new(*b"DsdM"));

/// Build a Symphonia `Probe` with all default formats plus the DSF format reader.
pub fn build_probe() -> Probe {
    let mut probe = Probe::default();
    symphonia::default::register_enabled_formats(&mut probe);
    probe.register_format::<DsfReader<'_>>();
    probe
}

/// Build a Symphonia `CodecRegistry` with all default codecs plus the DSD decoder.
pub fn build_codec_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<DsdDecoder>();
    registry
}
