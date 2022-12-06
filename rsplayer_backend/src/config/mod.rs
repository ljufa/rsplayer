use api_models::common::Volume;
use api_models::settings::Settings;
use api_models::state::{AudioOut, StreamerState};
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};

use crate::audio_device::alsa::AlsaPcmCard;

const SETTINGS_KEY: &str = "settings";
const STREAMER_STATUS_KEY: &str = "streamer_status";

#[cfg(debug_assertions)]
const CONFIG_DIR_PATH: &str = ".run/";
#[cfg(not(debug_assertions))]
const CONFIG_DIR_PATH: &str = "./";

#[cfg(debug_assertions)]
const EXEC_DIR_PATH: &str = ".run/";
#[cfg(not(debug_assertions))]
const EXEC_DIR_PATH: &str = "/usr/local/bin/";

pub struct Configuration {
    db: PickleDb,
}

impl Configuration {
    pub fn new() -> Configuration {
        if let Ok(db) = PickleDb::load(
            CONFIG_DIR_PATH.to_owned() + "configuration.db",
            PickleDbDumpPolicy::AutoDump,
            SerializationMethod::Json,
        ) {
            Configuration { db }
        } else {
            Configuration {
                db: PickleDb::new(
                    CONFIG_DIR_PATH.to_owned() + "configuration.db",
                    PickleDbDumpPolicy::AutoDump,
                    SerializationMethod::Json,
                ),
            }
        }
    }
    pub fn get_static_dir_path() -> String {
        format!("{}ui", CONFIG_DIR_PATH)
    }

    #[allow(dead_code)]
    pub fn get_squeezelite_player_path() -> String {
        format!("{}squeezelite", EXEC_DIR_PATH)
    }

    pub fn get_librespot_path() -> String {
        format!("{}librespot", EXEC_DIR_PATH)
    }

    pub fn get_streamer_status(&mut self) -> StreamerState {
        if let Some(ps) = self.db.get(STREAMER_STATUS_KEY) {
            ps
        } else {
            let default = StreamerState::default();
            self.db
                .set(STREAMER_STATUS_KEY, &default)
                .expect("Could not store default player state");
            default
        }
    }

    pub fn get_settings(&mut self) -> Settings {
        let mut result = if let Some(ds) = self.db.get(SETTINGS_KEY) {
            trace!("Existing settings config found: {:?}", ds);
            ds
        } else {
            info!("Existing configuration not found. Using default.");
            let default = Settings::default();
            self.db
                .set(SETTINGS_KEY, &default)
                .expect("Could not store default settings");
            default
        };
        result.alsa_settings.available_alsa_pcm_devices = AlsaPcmCard::get_all_cards();
        result
            .dac_settings
            .available_dac_chips
            .insert(String::from("AK4497"), String::from("AK4497"));
        result
            .dac_settings
            .available_dac_chips
            .insert(String::from("AK4490"), String::from("AK4490"));

        result
    }

    pub fn save_settings(&mut self, settings: &Settings) {
        self.db
            .set(SETTINGS_KEY, settings)
            .expect("Failed to store settings");
    }

    pub fn save_audio_output(&mut self, selected_output: AudioOut) -> StreamerState {
        let mut sstate = self.get_streamer_status();
        sstate.selected_audio_output = selected_output;
        self.save_streamer_state(&sstate);
        sstate.clone()
    }

    pub fn save_volume_state(&mut self, volume: Volume) -> StreamerState {
        let mut ss = self.get_streamer_status();
        ss.volume_state = volume;
        self.save_streamer_state(&ss);
        ss
    }

    fn save_streamer_state(&mut self, streamer_status: &StreamerState) {
        self.db
            .set(STREAMER_STATUS_KEY, streamer_status)
            .expect("Can't store new player state");
    }
}
