use std::fs::{File, OpenOptions};


use std::sync::{Arc, Mutex};

use api_models::common::Volume;

use api_models::settings::Settings;
use api_models::state::{AudioOut, StreamerState};
use log::warn;
use serde::{Deserialize, Serialize};

#[cfg(debug_assertions)]
const EXEC_DIR_PATH: &str = "./";
#[cfg(not(debug_assertions))]
const EXEC_DIR_PATH: &str = "/usr/local/bin/";

pub type MutArcConfiguration = Arc<Mutex<Configuration>>;

#[derive(Deserialize, Serialize)]
pub struct Configuration {
    settings: Settings,
    streamer_state: StreamerState,
}

impl Configuration {
    pub fn new() -> Self {
    _ = OpenOptions::new()
            .write(true)
            .create(true)
            .open("configuration.yaml")
            .expect("Faied to open configuration file");
        
        serde_yaml::from_reader(File::open("configuration.yaml").expect("")).unwrap_or_else(|e| {
            warn!("Failed to deserilize configuration file: {e}");
            let configuration = Configuration {
                settings: Settings::default(),
                streamer_state: StreamerState::default(),
            };
            write_to_file(&configuration);
            configuration
        })
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

    pub fn get_streamer_status(&self) -> StreamerState {
        self.streamer_state.clone()
    }

    pub fn get_settings(&self) -> Settings {
        let mut result = self.settings.clone();
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
        self.settings = settings.clone();
        write_to_file(self);
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
        self.streamer_state = streamer_status.clone();
        write_to_file(self);
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self::new()
    }
}

fn write_to_file(config: &Configuration) {
    let config_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open("configuration.yaml")
        .expect("Faied to open configuration file");
    _ = serde_yaml::to_writer(config_file, config);
}
