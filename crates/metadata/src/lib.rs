//! Music library: scanning, storage, queue, playlists, radio and loudness.
//!
//! Persistence is fjall (LSM key-value store), one `Database` shared by the
//! whole process with one keyspace per concern (`songs`, `albums`, `queue`,
//! `playlist`, `play_statistics`, `loudness`…). Songs are keyed by
//! library-relative file path; albums by normalized `artist|album`.
//!
//! Layout: `metadata_service` — scanner and library queries;
//! `queue_service` — the playback queue; `playlist_service` — saved
//! playlists; `*_repository` — fjall implementations of the `ports` traits
//! (kept behind traits so tests can use `ports::fakes`); `loudness_*` —
//! EBU R128 analysis for volume normalization; `icy_reader`/`radio_*` —
//! internet-radio metadata; `*_bundle` — custom Symphonia format/codec
//! plugins (APE, DSF/DSD, SACD ISO) registered via [`build_probe`] and
//! [`build_codec_registry`], which the playback crate also uses.

pub mod album_repository;
pub mod ape_bundle;
pub mod audio_metadata_extractor;
pub mod dsd_bundle;
pub mod error;
pub mod genre_utils;
pub mod icy_reader;
pub mod loudness_analyzer;
pub mod loudness_repository;
pub mod loudness_service;
pub mod metadata_service;
pub mod play_statistic_repository;
pub mod playlist_service;
pub mod ports;
pub mod queue_service;
pub mod radio_meta;
pub mod radio_providers;
pub mod sacd_bundle;
pub mod song_repository;
#[cfg(test)]
mod test;
use crate::ape_bundle::{ApeDecoder, ApeReader};
use crate::dsd_bundle::{DsdDecoder, DsfReader};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::formats::probe::Probe;

/// Build a Symphonia `Probe` with all default formats plus the DSF and APE format readers.
pub fn build_probe() -> Probe {
    let mut probe = Probe::default();
    symphonia::default::register_enabled_formats(&mut probe);
    probe.register_format::<DsfReader<'_>>();
    probe.register_format::<ApeReader>();
    probe
}

/// Build a Symphonia `CodecRegistry` with all default codecs plus the DSD and APE decoders.
pub fn build_codec_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<DsdDecoder>();
    registry.register_audio_decoder::<ApeDecoder>();
    registry
}
