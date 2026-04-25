pub mod album_repository;
pub mod ape_bundle;
pub mod audio_metadata_extractor;
pub mod dsd_bundle;
pub mod genre_utils;
pub mod icy_reader;
pub mod loudness_analyzer;
pub mod loudness_repository;
pub mod loudness_service;
pub mod metadata_service;
pub mod play_statistic_repository;
pub mod playlist_service;
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
