use std::sync::Arc;

use api_models::common::Volume;

use api_models::settings::Settings;
use api_models::state::StreamerState;
use sled::{Db, IVec};

const SETTINGS_KEY: &str = "settings";
const STATE_KEY: &str = "state";

pub type ArcConfiguration = Arc<Configuration>;

pub struct Configuration {
    db: Db,
}

impl Configuration {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let db = sled::open("configuration.db").expect("Failed to open configuration db");
        _ = db.compare_and_swap(
            SETTINGS_KEY,
            None as Option<IVec>,
            Some(IVec::from(serde_json::to_vec(&Settings::default()).unwrap())),
        );
        _ = db.compare_and_swap(
            STATE_KEY,
            None as Option<IVec>,
            Some(IVec::from(serde_json::to_vec(&StreamerState::default()).unwrap())),
        );
        Self { db }
    }

    pub fn get_settings(&self) -> Settings {
        let sett = self.db.get(SETTINGS_KEY).unwrap().unwrap();
        let result: Settings = serde_json::from_slice(&sett).unwrap();
        result
    }

    pub fn save_settings(&self, settings: &Settings) {
        _ = self.db.insert(SETTINGS_KEY, serde_json::to_vec(&settings).unwrap());
        _ = self.db.flush();
    }

    fn save_streamer_state(&self, streamer_status: &StreamerState) {
        _ = self.db.insert(STATE_KEY, serde_json::to_vec(streamer_status).unwrap());
    }

    pub fn get_streamer_state(&self) -> StreamerState {
        let state = self.db.get(STATE_KEY).unwrap().unwrap();
        serde_json::from_slice(&state).unwrap()
    }

    pub fn save_volume_state(&self, volume: Volume) -> StreamerState {
        let mut ss = self.get_streamer_state();
        ss.volume_state = volume;
        self.save_streamer_state(&ss);
        ss
    }
}

pub fn get_static_dir_path() -> String {
    "ui".to_string()
}
