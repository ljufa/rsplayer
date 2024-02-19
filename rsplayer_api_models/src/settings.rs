use std::collections::HashMap;

use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};
use validator::Validate;

use crate::common::{AudioCard, CardMixer, FilterType, GainLevel, PcmOutputDevice, VolumeCrtlType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub volume_ctrl_settings: VolumeControlSettings,
    pub output_selector_settings: OutputSelectorSettings,
    pub dac_settings: DacSettings,
    pub alsa_settings: AlsaSettings,
    pub ir_control_settings: IRInputControlerSettings,
    pub oled_settings: OLEDSettings,
    #[serde(default)]
    pub auto_resume_playback: bool,
    pub metadata_settings: MetadataStoreSettings,
    pub playback_queue_settings: PlaybackQueueSetting,
    #[serde(default)]
    pub playlist_settings: PlaylistSetting,
    #[serde(default)]
    pub rs_player_settings: RsPlayerSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RsPlayerSettings {
    pub enabled: bool,
    pub buffer_size_mb: usize,
}

impl Default for RsPlayerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size_mb: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSelectorSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeControlSettings {
    pub volume_step: u8,
    pub ctrl_device: VolumeCrtlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alsa_mixer: Option<CardMixer>,
    pub rotary_enabled: bool,
    pub rotary_event_device_path: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    FromPrimitive,
    ToPrimitive,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
pub enum AlsaDeviceFormat {
    F64,
    F32,
    S32,
    S24,
    S24_3,
    S16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MetadataStoreSettings {
    pub music_directory: String,
    pub follow_links: bool,
    pub supported_extensions: Vec<String>,
    pub db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaybackQueueSetting {
    pub db_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaylistSetting {
    pub db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlsaSettings {
    #[serde(default)]
    pub output_device: PcmOutputDevice,
    #[serde(default)]
    pub available_audio_cards: Vec<AudioCard>,
}
impl AlsaSettings {
    pub fn find_pcms_by_card_index(&self, card_index: i32) -> Vec<PcmOutputDevice> {
        self.available_audio_cards
            .iter()
            .find(|card| card.index == card_index)
            .map(|c| c.pcm_devices.clone())
            .unwrap_or_default()
    }

    pub fn set_output_device(&mut self, card_index: i32, pcm_name: &str) {
        if let Some(pcm) = self
            .find_pcms_by_card_index(card_index)
            .iter()
            .find(|pcm| pcm.name == pcm_name)
        {
            self.output_device = pcm.clone();
        }
    }

    pub fn find_mixers_by_card_index(&self, card_index: i32) -> Vec<CardMixer> {
        self.available_audio_cards
            .iter()
            .find(|card| card.index == card_index)
            .map(|mix| mix.mixers.clone())
            .unwrap_or_default()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DacSettings {
    pub enabled: bool,
    pub chip_id: String,
    pub i2c_address: u16,
    pub volume_step: u8,
    pub filter: FilterType,
    pub sound_sett: u8,
    pub gain: GainLevel,
    pub heavy_load: bool,
    #[serde(skip_deserializing)]
    pub available_dac_chips: HashMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRInputControlerSettings {
    pub enabled: bool,
    pub remote_maker: String,
    pub input_socket_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OLEDSettings {
    pub enabled: bool,
    pub display_model: String,
    pub spi_device_path: String,
}

impl Default for MetadataStoreSettings {
    fn default() -> Self {
        Self {
            music_directory: "/music".into(),
            follow_links: true,
            supported_extensions: vec!["flac", "wav", "mp3", "m4a", "aac", "aiff", "alac", "ogg", "wma", "mp4"]
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
pub const DEFAULT_ALSA_PCM_DEVICE: &str = "hw:0";
pub const DEFAULT_ALSA_MIXER: &str = "0,Master";

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_resume_playback: false,
            output_selector_settings: OutputSelectorSettings { enabled: false },
            volume_ctrl_settings: VolumeControlSettings {
                rotary_enabled: false,
                alsa_mixer: None,
                volume_step: 2,
                ctrl_device: VolumeCrtlType::Alsa,
                rotary_event_device_path: "/dev/input/by-path/platform-rotary@f-event".to_string(),
            },
            dac_settings: DacSettings {
                enabled: false,
                chip_id: String::from("AK4497"),
                i2c_address: 0x13,
                volume_step: 2,
                filter: FilterType::SharpRollOff,
                gain: GainLevel::V375,
                heavy_load: false,
                sound_sett: 5,
                available_dac_chips: HashMap::new(),
            },
            metadata_settings: MetadataStoreSettings::default(),
            playback_queue_settings: PlaybackQueueSetting::default(),
            alsa_settings: AlsaSettings {
                output_device: PcmOutputDevice::default(),
                available_audio_cards: vec![],
            },
            ir_control_settings: IRInputControlerSettings {
                enabled: false,
                remote_maker: "Apple_A1156".to_string(),
                input_socket_path: String::from("/var/run/lirc/lircd"),
            },
            oled_settings: OLEDSettings {
                enabled: false,
                display_model: "ST7920 - 128x64".to_string(),
                spi_device_path: "/dev/spidev0.0".to_string(),
            },
            playlist_settings: PlaylistSetting::default(),
            rs_player_settings: RsPlayerSettings::default(),
        }
    }
}
