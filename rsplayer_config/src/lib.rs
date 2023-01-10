use std::sync::{Arc, Mutex};

use api_models::common::Volume;
use api_models::settings::Settings;
use api_models::state::{AudioOut, StreamerState};
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};

const SETTINGS_KEY: &str = "settings";
const STREAMER_STATUS_KEY: &str = "streamer_status";

#[cfg(debug_assertions)]
const EXEC_DIR_PATH: &str = "./";
#[cfg(not(debug_assertions))]
const EXEC_DIR_PATH: &str = "/usr/local/bin/";

pub type MutArcConfiguration = Arc<Mutex<Configuration>>;

pub struct Configuration {
    db: PickleDb,
}

impl Configuration {
    pub fn new() -> Self {
        PickleDb::load(
            "configuration.db",
            PickleDbDumpPolicy::AutoDump,
            SerializationMethod::Json,
        )
        .map_or_else(
            |_| Self {
                db: PickleDb::new(
                    "configuration.db",
                    PickleDbDumpPolicy::AutoDump,
                    SerializationMethod::Json,
                ),
            },
            |db| Self { db },
        )
    }
    pub fn get_static_dir_path() -> String {
        "ui".to_string()
    }

    #[allow(dead_code)]
    pub fn get_squeezelite_player_path() -> String {
        format!("{EXEC_DIR_PATH}squeezelite")
    }

    pub fn get_librespot_path() -> String {
        format!("{EXEC_DIR_PATH}librespot")
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
            log::trace!("Existing settings config found: {:?}", ds);
            ds
        } else {
            log::info!("Existing configuration not found. Using default.");
            let default = Settings::default();
            self.db
                .set(SETTINGS_KEY, &default)
                .expect("Could not store default settings");
            default
        };
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

impl Default for Configuration {
    fn default() -> Self {
        Self::new()
    }
}
