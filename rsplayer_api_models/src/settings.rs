use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::common::{AudioCard, CardMixer, PcmOutputDevice, VolumeCrtlType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct Settings {
    pub volume_ctrl_settings: VolumeControlSettings,
    #[validate]
    pub alsa_settings: AlsaSettings,
    #[serde(default)]
    pub auto_resume_playback: bool,
    pub metadata_settings: MetadataStoreSettings,
    pub playback_queue_settings: PlaybackQueueSetting,
    #[serde(default)]
    pub playlist_settings: PlaylistSetting,
    #[serde(default)]
    #[validate]
    pub rs_player_settings: RsPlayerSettings,
    #[serde(default)]
    #[validate]
    pub uart_settings: UartCmdChannelSettings,
    #[serde(default)]
    #[validate]
    pub mqtt_settings: MqttCmdChannelSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
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
    #[serde(default = "player_state_db_path_default_value")]
    pub db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct UartCmdChannelSettings {
    pub enabled: bool,
    pub uart_path: String,
    pub baud_rate: u32,
    #[serde(default)]
    pub available_serial_devices: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]

pub struct MqttCmdChannelSettings {
    pub enabled: bool,
    pub mqtt_broker: String,
    pub mqtt_port: u16,
    pub mqtt_user: String,
    pub mqtt_password: String,
    pub mqtt_out_topic: String,
    pub mqtt_in_topic: String,
}
const fn thread_priority_default_value() -> u8 {
    1
}
const fn ring_buffer_size_default_value() -> usize {
    200
}
const fn input_stream_buffer_size_default_value() -> usize {
    10
}

fn player_state_db_path_default_value() -> String {
    "player_state".to_string()
}

impl Default for RsPlayerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_stream_buffer_size_mb: 10,
            ring_buffer_size_ms: 200,
            player_threads_priority: 1,
            alsa_buffer_size: None,
            db_path: player_state_db_path_default_value(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeControlSettings {
    pub volume_step: u8,
    pub ctrl_device: VolumeCrtlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alsa_mixer: Option<CardMixer>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct AlsaSettings {
    #[serde(default)]
    #[validate]
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

impl Default for UartCmdChannelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            uart_path: "/dev/ttyAMA0".to_string(),
            baud_rate: 115_200,
            available_serial_devices: vec![],
        }
    }
}
impl Default for MqttCmdChannelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mqtt_broker: "localhost".to_string(),
            mqtt_port: 1883,
            mqtt_user: "".to_string(),
            mqtt_password: "".to_string(),
            mqtt_out_topic: "rsplayer/out".to_string(),
            mqtt_in_topic: "rsplayer/in".to_string(),
        }
    }
}

pub const DEFAULT_ALSA_PCM_DEVICE: &str = "hw:0";
pub const DEFAULT_ALSA_MIXER: &str = "0,Master";

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_resume_playback: false,
            volume_ctrl_settings: VolumeControlSettings {
                alsa_mixer: None,
                volume_step: 2,
                ctrl_device: VolumeCrtlType::Alsa,
            },
            metadata_settings: MetadataStoreSettings::default(),
            playback_queue_settings: PlaybackQueueSetting::default(),
            alsa_settings: AlsaSettings {
                output_device: PcmOutputDevice::default(),
                available_audio_cards: vec![],
            },
            playlist_settings: PlaylistSetting::default(),
            rs_player_settings: RsPlayerSettings::default(),
            uart_settings: UartCmdChannelSettings::default(),
            mqtt_settings: MqttCmdChannelSettings::default(),
        }
    }
}
