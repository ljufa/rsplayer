use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use validator::{validate_ip_v4, Validate, ValidationError};

use crate::common::{FilterType, GainLevel, PlayerType, VolumeCrtlType, Command};

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
    #[validate(length(min = 3))]
    pub alsa_device_name: String,
    pub bitrate: u16,
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
    #[serde(skip_deserializing)]
    pub available_alsa_control_devices: HashMap<String, String>,
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
pub struct RemoteKeyMapping {
    maker: String,
    mappings: HashMap<String, Command>

}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OLEDSettings {
    pub enabled: bool,
    pub display_model: String,
    pub spi_device_path: String,
}

impl LmsSettings {
    pub fn get_cli_url(&self) -> String {
        format!("{}:{}", self.server_host, self.cli_port)
    }
}
impl MpdSettings {
    pub fn get_server_url(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
impl Default for Settings {
    fn default() -> Self {
        let default_alsa_pcm_device = "hw:1";
        Settings {
            active_player: PlayerType::MPD,
            output_selector_settings: OutputSelectorSettings { enabled: true },
            volume_ctrl_settings: VolumeControlSettings {
                volume_step: 2,
                ctrl_device: VolumeCrtlType::Dac,
                rotary_enabled: true,
                rotary_event_device_path: "/dev/input/by-path/platform-rotary@f-event".to_string(),
            },
            spotify_settings: SpotifySettings {
                enabled: false,
                device_name: String::from("rsplayer@rpi"),
                auth_callback_url: String::from("http://rsplayer.lan:8000/api/spotify/callback"),
                developer_client_id: String::default(),
                developer_secret: String::default(),
                username: String::default(),
                password: String::default(),
                alsa_device_name: format!("plug{}", default_alsa_pcm_device),
                bitrate: 320,
            },
            lms_settings: LmsSettings {
                enabled: false,
                server_host: String::from("localhost"),
                cli_port: 9090,
                server_port: 9000,
                alsa_pcm_device_name: String::from(default_alsa_pcm_device),
            },
            dac_settings: DacSettings {
                enabled: true,
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
                enabled: true,
                server_host: String::from("127.0.0.1"),
                server_port: 6600,
            },
            alsa_settings: AlsaSettings {
                device_name: String::from(default_alsa_pcm_device),
                available_alsa_pcm_devices: HashMap::new(),
                available_alsa_control_devices: HashMap::new(),
            },
            ir_control_settings: IRInputControlerSettings {
                enabled: true,
                remote_maker: "rsplayer".to_string(),
                input_socket_path: String::from("/var/run/lirc/lircd"),
            },
            oled_settings: OLEDSettings {
                enabled: false,
                display_model: "ST7920 - 128x64".to_string(),
                spi_device_path: "/dev/spidev0.0".to_string(),
            },
        }
    }
}
