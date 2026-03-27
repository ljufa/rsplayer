use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;

use api_models::settings::RsPlayerSettings;

#[allow(dead_code)]
pub struct PlaybackConfig {
    #[allow(dead_code)]
    pub stop_signal: Arc<AtomicBool>,
    #[allow(dead_code)]
    pub skip_to_time: Arc<AtomicU16>,
    pub audio_device: String,
    pub settings: RsPlayerSettings,
    pub music_dirs: Vec<String>,
    #[allow(dead_code)]
    pub vu_meter_enabled: bool,
    #[allow(dead_code)]
    pub is_local_browser_playback: bool,
}

impl PlaybackConfig {
    pub fn new(
        audio_device: String,
        settings: RsPlayerSettings,
        music_dirs: Vec<String>,
        vu_meter_enabled: bool,
        is_local_browser_playback: bool,
    ) -> Self {
        Self {
            stop_signal: Arc::new(AtomicBool::new(false)),
            skip_to_time: Arc::new(AtomicU16::new(0)),
            audio_device,
            settings,
            music_dirs,
            vu_meter_enabled,
            is_local_browser_playback,
        }
    }

    #[allow(dead_code)]
    pub const fn from_existing(
        stop_signal: Arc<AtomicBool>,
        skip_to_time: Arc<AtomicU16>,
        audio_device: String,
        settings: RsPlayerSettings,
        music_dirs: Vec<String>,
        vu_meter_enabled: bool,
        is_local_browser_playback: bool,
    ) -> Self {
        Self {
            stop_signal,
            skip_to_time,
            audio_device,
            settings,
            music_dirs,
            vu_meter_enabled,
            is_local_browser_playback,
        }
    }

    #[allow(dead_code)]
    pub fn is_stopped(&self) -> bool {
        self.stop_signal.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn get_skip_time(&self) -> u16 {
        self.skip_to_time.swap(0, Ordering::Relaxed)
    }
}
