use std::collections::HashMap;

use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};
use validator::{validate_ip_v4, Validate, ValidationError};

use crate::common::{FilterType, GainLevel, PlayerType, VolumeCrtlType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub volume_ctrl_settings: VolumeControlSettings,
    pub output_selector_settings: OutputSelectorSettings,
    pub spotify_settings: SpotifySettings,
    pub lms_settings: LmsSettings,
    pub mpd_settings: MpdSettings,
    pub dac_settings: DacSettings,
    pub alsa_settings: AlsaSettings,
    pub ir_control_settings: IRInputControlerSettings,
    pub oled_settings: OLEDSettings,
    pub active_player: PlayerType,
    pub metadata_settings: MetadataStoreSettings,
    pub playback_queue_settings: PlaybackQueueSetting,
    // pub playlist_settings: PlaylistSetting
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSelectorSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeControlSettings {
    pub volume_step: u8,
    pub ctrl_device: VolumeCrtlType,
    pub rotary_enabled: bool,
    pub rotary_event_device_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct SpotifySettings {
    pub enabled: bool,
    #[validate(length(min = 3))]
    pub device_name: String,
    #[validate(email)]
    pub username: String,
    #[validate(length(min = 3))]
    pub password: String,
    #[validate(length(min = 3))]
    pub developer_client_id: String,
    #[validate(length(min = 3))]
    pub developer_secret: String,
    #[validate(url)]
    pub auth_callback_url: String,
    pub bitrate: u16,
    pub alsa_device_format: AlsaDeviceFormat,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LmsSettings {
    pub enabled: bool,
    pub cli_port: u32,
    pub server_host: String,
    pub server_port: u32,
    pub alsa_pcm_device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MpdSettings {
    pub enabled: bool,
    #[validate(custom(function = "validate_ip"))]
    pub server_host: String,
    #[validate(range(min = 1024, max = 65535))]
    pub server_port: u32,
    pub override_external_configuration: bool,
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

fn validate_ip(val: &str) -> Result<(), ValidationError> {
    if validate_ip_v4(val) {
        Ok(())
    } else {
        Err(ValidationError::new("server_host"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlsaSettings {
    pub device_name: String,
    pub available_alsa_pcm_devices: HashMap<String, String>,
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

impl LmsSettings {
    #[must_use]
    pub fn get_cli_url(&self) -> String {
        format!("{}:{}", self.server_host, self.cli_port)
    }
}
impl MpdSettings {
    #[must_use]
    pub fn get_server_url(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

impl Default for MetadataStoreSettings {
    fn default() -> Self {
        Self {
            music_directory: "/var/lib/mpd/music".into(),
            follow_links: true,
            supported_extensions: vec![
                "flac", "wav", "mp3", "m4a", "aac", "aiff", "alac", "ogg", "wv", "wma", "mp4",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect(),
            db_path: "metadata.db".to_string(),
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
pub const DEFAULT_ALSA_PCM_DEVICE: &str = "hw:1";

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_player: PlayerType::RSP,
            output_selector_settings: OutputSelectorSettings { enabled: false },
            volume_ctrl_settings: VolumeControlSettings {
                rotary_enabled: false,
                volume_step: 2,
                ctrl_device: VolumeCrtlType::Dac,
                rotary_event_device_path: "/dev/input/by-path/platform-rotary@f-event".to_string(),
            },
            spotify_settings: SpotifySettings {
                enabled: false,
                device_name: String::from("rsplayer@rpi"),
                auth_callback_url: String::from("http://rsplayer.local/api/spotify/callback"),
                developer_client_id: String::default(),
                developer_secret: String::default(),
                username: String::default(),
                password: String::default(),
                alsa_device_format: AlsaDeviceFormat::S16,
                bitrate: 320,
            },
            lms_settings: LmsSettings {
                enabled: false,
                server_host: String::from("localhost"),
                cli_port: 9090,
                server_port: 9000,
                alsa_pcm_device_name: String::from(DEFAULT_ALSA_PCM_DEVICE),
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
            mpd_settings: MpdSettings {
                enabled: false,
                server_host: String::from("127.0.0.1"),
                server_port: 6600,
                override_external_configuration: false,
            },
            metadata_settings: MetadataStoreSettings::default(),
            playback_queue_settings: PlaybackQueueSetting::default(),
            alsa_settings: AlsaSettings {
                device_name: String::from(DEFAULT_ALSA_PCM_DEVICE),
                available_alsa_pcm_devices: HashMap::new(),
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
            // playlist_settings: PlaylistSetting::default()
        }
    }
}
