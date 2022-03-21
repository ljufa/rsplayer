use std::{
    env, fs,
    path::{Path, PathBuf},
};

use api_models::settings::Settings;
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};

use api_models::player::*;

const SETTINGS_KEY: &str = "settings";
const STREAMER_STATUS_KEY: &str = "streamer_status";
const DPLAY_CONFIG_DIR_PATH: &str = ".dplay/";
const CACHE_FILE: &str = ".spotify_token_cache";

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
    pub fn get_static_dir_path() -> String {
        format!("{}ui", DPLAY_CONFIG_DIR_PATH)
    }
    pub fn get_squeezelite_player_path() -> String {
        format!("{}squeezelite", DPLAY_CONFIG_DIR_PATH)
    }
    pub fn get_librespot_path() -> String {
        format!("{}librespot", DPLAY_CONFIG_DIR_PATH)
    }

    pub fn spotify_cache_path() -> PathBuf {
        let project_dir_path = env::current_dir().unwrap();
        let mut cache_path = PathBuf::from(project_dir_path);
        cache_path.push(DPLAY_CONFIG_DIR_PATH);
        let cache_dir = cache_path.display().to_string();
        cache_path.push(CACHE_FILE);
        if !Path::new(cache_dir.as_str()).exists() {
            fs::create_dir_all(cache_dir).unwrap();
        }
        cache_path
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
            debug!("Existing settings config found: {:?}", ds);
            result = ds;
        } else {
            info!("Existing configuration not found. Using default.");
            let default = Settings::default();
            self.db
                .set(SETTINGS_KEY, &default)
                .expect("Could not store default settings");
            result = default;
        }
        result.alsa_settings.available_alsa_pcm_devices =
            crate::audio_device::alsa::get_all_cards();
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
