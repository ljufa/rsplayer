//! Opus support as a Symphonia codec plugin, backed by the pure-Rust
//! [`opus_rs`] decoder.
//!
//! Symphonia's demuxers already handle the containers (Ogg, Matroska/WebM,
//! MP4) and hand over the Opus identification header as extra data, but the
//! upstream crate ships no Opus decoder. [`OpusDecoder`] fills that gap and is
//! registered by [`crate::build_codec_registry`].

pub mod decoder;

pub use decoder::OpusDecoder;
