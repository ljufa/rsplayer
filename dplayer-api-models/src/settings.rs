use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub spotify_settings: SpotifySettings,
    pub lms_settings: LmsSettings,
    pub mpd_settings: MpdSettings,
    pub dac_settings: DacSettings,
    pub alsa_settings: AlsaSettings,
    pub ir_control_settings: IRInputControlerSettings,
    pub oled_settings: OLEDSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifySettings {
    pub enabled: bool,
    pub device_name: String,
    pub username: String,
    pub password: String,
    #[serde(skip_deserializing)]
    pub developer_client_id: String,
    #[serde(skip_deserializing)]
    pub developer_secret: String,
    #[serde(skip_deserializing)]
    pub auth_callback_url: String,
    #[serde(skip_deserializing)]
    pub alsa_device_name: String,
    #[serde(skip_deserializing)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpdSettings {
    pub enabled: bool,
    pub server_host: String,
    pub server_port: u32,
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
    #[serde(skip_deserializing)]
    pub available_dac_chips: HashMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRInputControlerSettings {
    pub enabled: bool,
    pub input_socket_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OLEDSettings {
    pub enabled: bool,
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
            spotify_settings: SpotifySettings {
                enabled: false,
                device_name: String::from("dplayer@rpi"),
                auth_callback_url: String::from("http://dplayer.lan:8000"),
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
                available_dac_chips: HashMap::new(),
            },
            mpd_settings: MpdSettings {
                enabled: true,
                server_host: String::from("localhost"),
                server_port: 6677,
            },
            alsa_settings: AlsaSettings {
                device_name: String::from(default_alsa_pcm_device),
                available_alsa_pcm_devices: HashMap::new(),
                available_alsa_control_devices: HashMap::new(),
            },
            ir_control_settings: IRInputControlerSettings {
                enabled: true,
                input_socket_path: String::from("/var/run/lirc/lircd"),
            },
            oled_settings: OLEDSettings { enabled: false },
        }
    }
}
