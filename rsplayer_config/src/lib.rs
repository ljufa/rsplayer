
use std::fs::{File, OpenOptions};

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use api_models::common::Volume;

use api_models::settings::Settings;
use api_models::state::{AudioOut, StreamerState};
use log::info;
use serde::{Deserialize, Serialize};

#[cfg(debug_assertions)]
const EXEC_DIR_PATH: &str = "./";
#[cfg(not(debug_assertions))]
const EXEC_DIR_PATH: &str = "/usr/local/bin/";

const CONFIG_FILE_NAME: &str = "configuration.yaml";

pub type MutArcConfiguration = Arc<Mutex<Configuration>>;

#[derive(Deserialize, Serialize, Default)]
pub struct Configuration {
    settings: Settings,
    streamer_state: StreamerState,
}

impl Configuration {
    pub fn new() -> Self {
        create_default_config_file_if_not_exist();
        serde_yaml::from_reader(File::open(CONFIG_FILE_NAME).expect("")).unwrap_or_else(|e| {
            panic!("Failed to deserilize configuration file: {e}");
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
    }
}


impl Drop for Configuration {
    fn drop(&mut self) {
        self.settings.alsa_settings.available_alsa_pcm_devices.clear();
        write_to_file(self, false);
    }
}

fn create_default_config_file_if_not_exist() {
    let fpath = Path::new(CONFIG_FILE_NAME);
    if !fpath.exists() {
        write_to_file(&mut Configuration::default(), true);
    }
}

fn write_to_file(config: &mut Configuration, is_new: bool) {
    let mut config_file = OpenOptions::new()
        .write(true)
        .append(false)
        .create(is_new)
        .open(CONFIG_FILE_NAME)
        .expect("Faied to open configuration file");
    _ = serde_yaml::to_string(config).map(|config_string|{
        info!("Save configuration to file:\n{config_string}");
        _ = config_file.write_all(config_string.as_bytes());
        _ = config_file.flush();
        info!("Configuration file saved!");
    });  

}
