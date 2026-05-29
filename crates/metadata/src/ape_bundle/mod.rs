pub mod decoder;
pub mod demuxer;

pub use decoder::ApeDecoder;
pub use demuxer::ApeReader;

use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::common::FourCc;

pub const CODEC_TYPE_APE: AudioCodecId = AudioCodecId::new(FourCc::new(*b"APE "));
