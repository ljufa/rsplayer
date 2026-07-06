//! DSD support as a Symphonia plugin.
//!
//! [`DsfReader`] demuxes `.dsf` files, [`DsdDecoder`] passes the raw 1-bit
//! stream through untouched — actual output happens in the playback crate's
//! DSD path (native DSD or `DoP`), never through the PCM/DSP chain.

pub mod decoder;
pub mod demuxer;
pub mod dsf;

pub use decoder::DsdDecoder;
pub use demuxer::DsfReader;

use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::common::FourCc;

/// DSD codec ID constants — defined in the user namespace via `FourCC`
/// since upstream symphonia-core does not include them.
pub const CODEC_TYPE_DSD_LSBF: AudioCodecId = AudioCodecId::new(FourCc::new(*b"DsdL"));
pub const CODEC_TYPE_DSD_MSBF: AudioCodecId = AudioCodecId::new(FourCc::new(*b"DsdM"));

pub use crate::ape_bundle::CODEC_TYPE_APE;
