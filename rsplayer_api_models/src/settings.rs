use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};
use validator::Validate;

use crate::common::{AudioCard, CardMixer, PcmOutputDevice, VolumeCrtlType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Settings {
    pub volume_ctrl_settings: VolumeControlSettings,
    #[validate(nested)]
    pub alsa_settings: AlsaSettings,
    #[serde(default)]
    pub auto_resume_playback: bool,
    pub metadata_settings: MetadataStoreSettings,
    pub playback_queue_settings: PlaybackQueueSetting,
    #[serde(default)]
    pub playlist_settings: PlaylistSetting,
    #[serde(default)]
    #[validate(nested)]
    pub rs_player_settings: RsPlayerSettings,
    #[serde(default)]
    #[validate(nested)]
    pub usb_settings: UsbCmdChannelSettings,
    #[serde(default)]
    #[validate(nested)]
    pub network_storage_settings: NetworkStorageSettings,
    #[serde(default)]
    pub local_browser_playback: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub demo_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NormalizationSource {
    /// Prefer file tags (track gain); fall back to `RSPlayer` EBU R128 calculated loudness.
    #[default]
    Auto,
    /// Use `REPLAYGAIN_TRACK_GAIN` or `R128_TRACK_GAIN` from the file, applied as-is.
    FileTagsTrack,
    /// Use `REPLAYGAIN_ALBUM_GAIN` or `R128_ALBUM_GAIN` from the file, applied as-is.
    FileTagsAlbum,
    /// Use `RSPlayer`'s own EBU R128 integrated loudness measurement (original behavior).
    Calculated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct RsPlayerSettings {
    pub enabled: bool,

    #[serde(default = "input_stream_buffer_size_default_value")]
    #[validate(range(min = 1, max = 200))]
    pub input_stream_buffer_size_mb: usize,

    #[serde(default = "ring_buffer_size_default_value")]
    #[validate(range(min = 100, max = 10000))]
    pub ring_buffer_size_ms: usize,

    #[serde(default = "thread_priority_default_value")]
    #[validate(range(min = 1, max = 99))]
    pub player_threads_priority: u8,
    pub alsa_buffer_size: Option<u32>,
    #[serde(default)]
    pub fixed_output_sample_rate: Option<u32>,
    #[serde(default)]
    #[validate(nested)]
    pub dsp_settings: DspSettings,
    #[serde(default = "default_vu_meter_enabled")]
    pub vu_meter_enabled: bool,
    #[serde(default)]
    pub loudness_normalization_enabled: bool,
    #[serde(default = "default_normalization_target_lufs")]
    pub loudness_normalization_target_lufs: f64,
    #[serde(default)]
    pub loudness_normalization_source: NormalizationSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DspSettings {
    #[serde(default = "default_dsp_enabled")]
    pub enabled: bool,
    #[serde(default)]
    #[validate(nested)]
    pub filters: Vec<FilterConfig>,
}

impl Default for DspSettings {
    fn default() -> Self {
        Self {
            enabled: default_dsp_enabled(),
            filters: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct FilterConfig {
    #[serde(flatten)]
    pub filter: DspFilter,
    #[serde(default)]
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DspFilter {
    Peaking {
        freq: f64,
        q: f64,
        gain: f64,
    },
    LowShelf {
        freq: f64,
        #[serde(default)]
        q: Option<f64>,
        #[serde(default)]
        slope: Option<f64>,
        gain: f64,
    },
    HighShelf {
        freq: f64,
        #[serde(default)]
        q: Option<f64>,
        #[serde(default)]
        slope: Option<f64>,
        gain: f64,
    },
    LowPass {
        freq: f64,
        q: f64,
    },
    HighPass {
        freq: f64,
        q: f64,
    },
    BandPass {
        freq: f64,
        q: f64,
    },
    Notch {
        freq: f64,
        q: f64,
    },
    AllPass {
        freq: f64,
        q: f64,
    },
    LowPassFO {
        freq: f64,
    },
    HighPassFO {
        freq: f64,
    },
    LowShelfFO {
        freq: f64,
        gain: f64,
    },
    HighShelfFO {
        freq: f64,
        gain: f64,
    },
    LinkwitzTransform {
        freq_act: f64,
        q_act: f64,
        freq_target: f64,
        q_target: f64,
    },
    Gain {
        gain: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, EnumIter, EnumString, IntoStaticStr)]
pub enum NetworkMountType {
    #[default]
    Smb,
    Nfs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct NetworkMountConfig {
    #[validate(length(min = 1))]
    pub name: String,
    pub mount_type: NetworkMountType,
    #[validate(length(min = 1))]
    pub server: String,
    #[validate(length(min = 1))]
    pub share: String,
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, Default)]
pub struct NetworkStorageSettings {
    #[serde(default)]
    pub mounts: Vec<NetworkMountConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct UsbCmdChannelSettings {
    pub enabled: bool,
    pub baud_rate: u32,
}

const fn thread_priority_default_value() -> u8 {
    1
}
const fn ring_buffer_size_default_value() -> usize {
    1000
}
const fn input_stream_buffer_size_default_value() -> usize {
    10
}
const fn default_dsp_enabled() -> bool {
    false
}
const fn default_vu_meter_enabled() -> bool {
    false
}
const fn default_normalization_target_lufs() -> f64 {
    -18.0
}

impl Default for RsPlayerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_stream_buffer_size_mb: 10,
            ring_buffer_size_ms: 1000,
            player_threads_priority: 1,
            alsa_buffer_size: None,
            fixed_output_sample_rate: None,
            dsp_settings: DspSettings::default(),
            vu_meter_enabled: default_vu_meter_enabled(),
            loudness_normalization_enabled: false,
            loudness_normalization_target_lufs: default_normalization_target_lufs(),
            loudness_normalization_source: NormalizationSource::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VolumeControlSettings {
    pub volume_step: u8,
    pub ctrl_device: VolumeCrtlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alsa_mixer_name: Option<String>,
    #[serde(skip)]
    pub alsa_mixer: Option<CardMixer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_volume: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_before_mute: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MetadataStoreSettings {
    /// Legacy field kept for backward compat deserialization only.
    #[serde(default, skip_serializing)]
    pub music_directory: String,
    #[serde(default)]
    pub music_directories: Vec<String>,
    pub follow_links: bool,
    #[serde(default, skip_serializing)]
    pub supported_extensions: Vec<String>,
    pub db_path: String,
}

impl MetadataStoreSettings {
    /// Returns the effective list of music directories.
    /// Falls back to the legacy single `music_directory` if `music_directories` is empty.
    pub fn effective_directories(&self) -> Vec<String> {
        if self.music_directories.is_empty() {
            if self.music_directory.is_empty() {
                vec![]
            } else {
                vec![self.music_directory.clone()]
            }
        } else {
            self.music_directories.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaybackQueueSetting {
    pub db_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaylistSetting {
    pub db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct AlsaSettings {
    #[serde(default)]
    #[validate(nested)]
    pub output_device: PcmOutputDevice,
    #[serde(default)]
    pub available_audio_cards: Vec<AudioCard>,
}
impl AlsaSettings {
    pub fn find_pcms_by_card_id(&self, card_id: &str) -> Vec<PcmOutputDevice> {
        self.available_audio_cards
            .iter()
            .find(|card| card.id == card_id)
            .map(|c| c.pcm_devices.clone())
            .unwrap_or_default()
    }

    pub fn set_output_device(&mut self, card_id: &str, pcm_name: &str) {
        if let Some(pcm) = self
            .find_pcms_by_card_id(card_id)
            .iter()
            .find(|pcm| pcm.name == pcm_name)
        {
            self.output_device = pcm.clone();
        }
    }

    pub fn find_mixers_by_card_id(&self, card_id: &str) -> Vec<CardMixer> {
        self.available_audio_cards
            .iter()
            .find(|card| card.id == card_id)
            .map(|mix| mix.mixers.clone())
            .unwrap_or_default()
    }
}

impl Default for MetadataStoreSettings {
    fn default() -> Self {
        Self {
            music_directory: String::new(),
            music_directories: vec![],
            follow_links: true,
            supported_extensions: vec![
                // Lossless
                "flac", "wav", "aiff", "aif", "ape", // Lossy
                "mp3", "mp2", "mp1", "m4a", "ogg", "oga", // Lossless containers
                "caf", // Matroska / WebM (audio-only containers)
                "mka", "weba", // DSD
                "dsf", "dff", // SACD disc image
                "iso",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect(),
            db_path: "ignored_files.db".to_string(),
        }
    }
}
impl Default for PlaybackQueueSetting {
    fn default() -> Self {
        Self {
            db_path: "queue.db".to_string(),
        }
    }
}
impl Default for PlaylistSetting {
    fn default() -> Self {
        Self {
            db_path: "playlist.db".to_string(),
        }
    }
}

impl Default for UsbCmdChannelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            baud_rate: 115_200,
        }
    }
}

pub const DEFAULT_ALSA_PCM_DEVICE: &str = "hw:0";
pub const DEFAULT_ALSA_MIXER: &str = "0,Master";

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_resume_playback: false,
            volume_ctrl_settings: VolumeControlSettings::default(),
            metadata_settings: MetadataStoreSettings::default(),
            playback_queue_settings: PlaybackQueueSetting::default(),
            alsa_settings: AlsaSettings {
                output_device: PcmOutputDevice::default(),
                available_audio_cards: vec![],
            },
            playlist_settings: PlaylistSetting::default(),
            rs_player_settings: RsPlayerSettings::default(),
            usb_settings: UsbCmdChannelSettings::default(),
            network_storage_settings: NetworkStorageSettings::default(),
            local_browser_playback: false,
            version: String::new(),
            demo_mode: false,
        }
    }
}
