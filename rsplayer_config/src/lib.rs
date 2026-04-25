use std::sync::{Arc, RwLock};

use api_models::settings::Settings;
use fjall::{Database, Keyspace, KeyspaceCreateOptions};

const SETTINGS_KEY: &str = "settings";

pub type ArcConfiguration = Arc<Configuration>;

pub struct Configuration {
    tree: Keyspace,
    settings: RwLock<Settings>,
}

impl Configuration {
    #[allow(clippy::new_without_default)]
    pub fn new(db: &Database) -> Self {
        let tree = db
            .keyspace("configuration", KeyspaceCreateOptions::default)
            .expect("Failed to open configuration keyspace");
        let settings = if let Ok(Some(data)) = tree.get(SETTINGS_KEY) {
            match serde_json::from_slice::<Settings>(&data) {
                Ok(mut settings) => {
                    let value: serde_json::Value = serde_json::from_slice(&data).unwrap_or_default();
                    if let Some(mixer_val) = value.get("volume_ctrl_settings").and_then(|v| v.get("alsa_mixer")) {
                        if mixer_val.is_object() {
                            if let Some(name) = mixer_val.get("name").and_then(|n| n.as_str()) {
                                settings.volume_ctrl_settings.alsa_mixer_name = Some(name.to_string());
                            }
                        }
                    }
                    // Migrate legacy single music_directory to music_directories
                    if settings.metadata_settings.music_directories.is_empty()
                        && !settings.metadata_settings.music_directory.is_empty()
                    {
                        settings.metadata_settings.music_directories =
                            vec![settings.metadata_settings.music_directory.clone()];
                        _ = tree.insert(
                            SETTINGS_KEY,
                            serde_json::to_vec(&settings).expect("failed to serialize settings"),
                        );
                        log::info!(
                            "Migrated legacy music_directory '{}' to music_directories",
                            settings.metadata_settings.music_directory
                        );
                    }
                    settings
                }
                Err(e) => {
                    log::error!("Failed to deserialize settings from DB: {e}. Falling back to default.");
                    Settings::default()
                }
            }
        } else {
            let s = Settings::default();
            _ = tree.insert(
                SETTINGS_KEY,
                serde_json::to_vec(&s).expect("failed to serialize settings"),
            );
            s
        };
        Self {
            tree,
            settings: RwLock::new(settings),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.read().expect("settings lock poisoned").clone()
    }

    pub fn get_settings_mut(&self) -> std::sync::RwLockWriteGuard<'_, Settings> {
        self.settings.write().expect("settings lock poisoned")
    }

    pub fn save_settings(&self, settings: &Settings) {
        *self.settings.write().expect("settings lock poisoned") = settings.clone();
        _ = self.tree.insert(
            SETTINGS_KEY,
            serde_json::to_vec(settings).expect("failed to serialize settings"),
        );
    }
}

pub fn get_static_dir_path() -> String {
    "ui".to_string()
}
