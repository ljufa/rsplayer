use std::sync::{Arc, RwLock};

use api_models::common::Volume;

use api_models::settings::Settings;
use api_models::state::StreamerState;
use sled::{Db, IVec};

const SETTINGS_KEY: &str = "settings";
const STATE_KEY: &str = "state";

pub type ArcConfiguration = Arc<Configuration>;

pub struct Configuration {
    db: Db,
    settings: RwLock<Settings>,
}

impl Configuration {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let db = sled::open("configuration.db").expect("Failed to open configuration db");
        let settings = if let Ok(Some(data)) = db.get(SETTINGS_KEY) {
            serde_json::from_slice(&data).unwrap_or_default()
        } else {
            let s = Settings::default();
            _ = db.insert(SETTINGS_KEY, IVec::from(serde_json::to_vec(&s).unwrap()));
            s
        };
        _ = db.compare_and_swap(
            STATE_KEY,
            None as Option<IVec>,
            Some(IVec::from(serde_json::to_vec(&StreamerState::default()).unwrap())),
        );
        Self {
            db,
            settings: RwLock::new(settings),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn get_settings_mut(&self) -> std::sync::RwLockWriteGuard<Settings> {
        self.settings.write().unwrap()
    }

    pub fn save_settings(&self, settings: &Settings) {
        *self.settings.write().unwrap() = settings.clone();
        _ = self.db.insert(SETTINGS_KEY, serde_json::to_vec(settings).unwrap());
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
