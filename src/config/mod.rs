use std::collections::HashMap;

use crate::common::{AudioOut, FilterType, PlayerType, StreamerStatus, DPLAY_CONFIG_DIR_PATH};
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};

const STREAMER_STATUS_KEY: &str = "streamer_status";
const SETTINGS_KEY: &str = "settings";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spotify_settings: Option<SpotifySettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lms_settings: Option<LmsSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpd_settings: Option<MpdSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dac_settings: Option<DacSettings>,
    pub alsa_settings: AlsaSettings,
    pub available_alsa_pcm_devices: HashMap<String, String>,
    pub available_alsa_control_devices: HashMap<String, String>,
    pub available_dac_chips: HashMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpotifySettings {
    pub device_name: String,
    pub developer_client_id: String,
    pub developer_secret: String,
    pub auth_callback_url: String,
    pub username: String,
    pub password: String,
    pub alsa_device_name: String,
    pub bitrate: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LmsSettings {
    pub cli_port: u32,
    pub server_host: String,
    pub server_port: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alsa_control_device_name: Option<String>,
    pub alsa_pcm_device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MpdSettings {
    pub server_host: String,
    pub server_port: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AlsaSettings {
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DacSettings {
    pub chip_id: String,
    pub i2c_address: u16,
    pub volume_step: u8,
}

impl LmsSettings {
    pub fn get_player_url(&self) -> String {
        format!("\"{}:{}\"", self.server_host, self.server_port)
    }
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
            spotify_settings: Some(SpotifySettings {
                device_name: String::from("dplayer@rpi"),
                auth_callback_url: String::from("http://dplayer.lan:8000"),
                developer_client_id: String::default(),
                developer_secret: String::default(),
                username: String::default(),
                password: String::default(),
                alsa_device_name: format!("plug{}", default_alsa_pcm_device),
                bitrate: 320,
            }),
            lms_settings: Some(LmsSettings {
                server_host: String::from("localhost"),
                cli_port: 9090,
                server_port: 9000,
                alsa_control_device_name: None,
                alsa_pcm_device_name: String::from(default_alsa_pcm_device),
            }),
            dac_settings: Some(DacSettings {
                chip_id: String::from("AK4497"),
                i2c_address: 0x13,
                volume_step: 2,
            }),
            mpd_settings: Some(MpdSettings {
                server_host: String::from("localhost"),
                server_port: 6600,
            }),
            alsa_settings: AlsaSettings {
                device_name: String::from(default_alsa_pcm_device),
            },
            available_alsa_pcm_devices: HashMap::new(),
            available_alsa_control_devices: HashMap::new(),
            available_dac_chips: HashMap::new(),
        }
    }
}

pub struct Configuration {
    db: PickleDb,
}

impl Configuration {
    pub fn new() -> Configuration {
        if let Ok(db) = PickleDb::load(
            DPLAY_CONFIG_DIR_PATH.to_owned() + "/configuration.db",
            PickleDbDumpPolicy::AutoDump,
            SerializationMethod::Json,
        ) {
            Configuration { db }
        } else {
            Configuration {
                db: PickleDb::new(
                    DPLAY_CONFIG_DIR_PATH.to_owned() + "/configuration.db",
                    PickleDbDumpPolicy::AutoDump,
                    SerializationMethod::Json,
                ),
            }
        }
    }

    pub fn get_streamer_status(&mut self) -> StreamerStatus {
        if let Some(ps) = self.db.get(STREAMER_STATUS_KEY) {
            ps
        } else {
            let default = StreamerStatus::default();
            self.db
                .set(STREAMER_STATUS_KEY, &default)
                .expect("Could not store default player state");
            default
        }
    }

    pub fn patch_streamer_status(
        &mut self,
        current_player: Option<PlayerType>,
        selected_output: Option<AudioOut>,
    ) -> StreamerStatus {
        let mut sstate = self.get_streamer_status();
        if let Some(c) = current_player {
            sstate.source_player = c;
        }
        if let Some(o) = selected_output {
            sstate.selected_audio_output = o;
        }
        self.save_streamer_status(&sstate);
        sstate.clone()
    }

    pub fn get_settings(&mut self) -> Settings {
        let mut result: Settings;
        if let Some(ds) = self.db.get(SETTINGS_KEY) {
            result = ds;
        } else {
            let default = Settings::default();
            self.db
                .set(SETTINGS_KEY, &default)
                .expect("Could not store default settings");
            result = default;
        }
        result.available_alsa_pcm_devices = crate::audio_device::alsa::get_all_cards();
        result
            .available_dac_chips
            .insert(String::from("AK4497"), String::from("AK4497"));
        result
            .available_dac_chips
            .insert(String::from("AK4490"), String::from("AK4490"));
        result
    }

    pub fn save_settings(&mut self, settings: &Settings) {
        self.db
            .set(SETTINGS_KEY, settings)
            .expect("Failed to store settings");
    }

    fn save_streamer_status(&mut self, streamer_status: &StreamerStatus) {
        self.db
            .set(STREAMER_STATUS_KEY, streamer_status)
            .expect("Can't store new player state");
    }

    pub fn patch_dac_status(
        &mut self,
        volume: Option<u8>,
        filter: Option<FilterType>,
        sound_sett: Option<u8>,
    ) -> StreamerStatus {
        let mut ss = self.get_streamer_status();
        let mut ds = ss.dac_status.clone();
        if let Some(v) = volume {
            ds.volume = v;
        }
        if let Some(f) = filter {
            ds.filter = f;
        }
        if let Some(ss) = sound_sett {
            ds.sound_sett = ss;
        }
        ss.dac_status = ds;
        self.save_streamer_status(&mut ss);
        trace!("New patched streamer status {:?}", ss);
        ss.clone()
    }
}
