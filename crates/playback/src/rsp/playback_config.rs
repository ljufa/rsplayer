//! Immutable per-track configuration handed to the decode loop (device
//! name, `RsPlayerSettings`, music dirs). Sibling of [`PlaybackContext`],
//! which carries the *mutable* control state.
//!
//! [`PlaybackContext`]: crate::rsp::playback_context::PlaybackContext

use api_models::settings::RsPlayerSettings;

pub struct PlaybackConfig {
    pub audio_device: String,
    pub settings: RsPlayerSettings,
    pub music_dirs: Vec<String>,
}

impl PlaybackConfig {
    pub const fn new(audio_device: String, settings: RsPlayerSettings, music_dirs: Vec<String>) -> Self {
        Self {
            audio_device,
            settings,
            music_dirs,
        }
    }
}
