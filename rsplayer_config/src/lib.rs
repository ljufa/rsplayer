use std::sync::{Arc, RwLock};

use api_models::settings::Settings;
use sled::{Db, IVec};

const SETTINGS_KEY: &str = "settings";

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
        Self {
            db,
            settings: RwLock::new(settings),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn get_settings_mut(&self) -> std::sync::RwLockWriteGuard<'_, Settings> {
        self.settings.write().unwrap()
    }

    pub fn save_settings(&self, settings: &Settings) {
        *self.settings.write().unwrap() = settings.clone();
        _ = self.db.insert(SETTINGS_KEY, serde_json::to_vec(settings).unwrap());
        _ = self.db.flush();
    }

}

pub fn get_static_dir_path() -> String {
    "ui".to_string()
}
