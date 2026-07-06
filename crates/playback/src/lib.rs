//! The audio engine: decoding, output, DSP hook-up, VU metering, DSD, and
//! the multiroom tee/sink. See `rsp/mod.rs` for the module map; the public
//! entry point is [`rsp::player_service::PlayerService`].

pub mod rsp;
