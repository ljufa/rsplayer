//! Immutable per-track configuration handed to the decode loop (device
//! name, `RsPlayerSettings`, library settings). Sibling of [`PlaybackContext`],
//! which carries the *mutable* control state.
//!
//! [`PlaybackContext`]: crate::rsp::playback_context::PlaybackContext

use api_models::settings::{MetadataStoreSettings, RsPlayerSettings};

pub struct PlaybackConfig {
    pub audio_device: String,
    pub settings: RsPlayerSettings,
    pub music_dirs: Vec<String>,
    /// Library settings, for resolving remote-library keys.
    pub library: MetadataStoreSettings,
}

impl PlaybackConfig {
    pub fn new(audio_device: String, settings: RsPlayerSettings, library: MetadataStoreSettings) -> Self {
        Self {
            audio_device,
            settings,
            music_dirs: library.effective_directories(),
            library,
        }
    }
}
