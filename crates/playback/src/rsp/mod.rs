//! Playback engine module map.
//!
//! Data flow for one track:
//! `player_service` (thread lifecycle, queue advance) → `symphonia`
//! (`play_file` decode loop) → `audio_output` (`AudioOutput`: ring buffer +
//! cpal stream, resampling, EQ, VU, software volume) → device.
//! `audio_source` resolves paths/URLs into probed readers; `dsd` bypasses
//! the PCM chain entirely; `tee`/`sync_sink` are the multiroom taps
//! (documented in `docs/multiroom_architecture.md`).

mod audio_output;
pub mod audio_host;
mod audio_source;
mod device_capabilities;
mod dsd;
mod playback_config;
mod playback_context;
pub mod player_service;
mod symphonia;
pub mod sync_sink;
pub mod tee;
mod vumeter;
