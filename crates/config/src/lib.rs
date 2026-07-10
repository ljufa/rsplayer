//! Settings persistence.
//!
//! [`Configuration`] owns the `configuration` fjall keyspace and an in-memory
//! `RwLock<Settings>` cache of the single JSON-serialized [`Settings`] value;
//! reads clone the cache, writes update cache and disk together. One-time
//! schema migrations (legacy `alsa_mixer` object, single `music_directory`)
//! run in [`Configuration::new`] when the stored JSON predates the current
//! model. On first launch (or unreadable stored settings) the caller-provided
//! platform-aware defaults are persisted instead of `Settings::default()`.

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
    /// Opens the configuration keyspace. `first_launch_settings` is persisted
    /// and used when no settings are stored yet (first launch) or the stored
    /// value can't be deserialized — callers pass platform-aware defaults so
    /// playback works before the user visits Settings.
    pub fn new(db: &Database, first_launch_settings: Settings) -> ArcConfiguration {
        let tree = db
            .keyspace("configuration", KeyspaceCreateOptions::default)
            .expect("Failed to open configuration keyspace");
        let settings = if let Ok(Some(data)) = tree.get(SETTINGS_KEY) {
            match serde_json::from_slice::<Settings>(&data) {
                Ok(mut settings) => {
                    let value: serde_json::Value = serde_json::from_slice(&data).unwrap_or_default();
                    if let Some(mixer_val) = value.get("volume_ctrl_settings").and_then(|v| v.get("alsa_mixer"))
                        && mixer_val.is_object()
                        && let Some(name) = mixer_val.get("name").and_then(|n| n.as_str())
                    {
                        settings.volume_ctrl_settings.alsa_mixer_name = Some(name.to_string());
                    }
                    // Migrate legacy single music_directory to music_directories
                    if settings.metadata_settings.music_directories.is_empty() && !settings.metadata_settings.music_directory.is_empty() {
                        settings.metadata_settings.music_directories = vec![settings.metadata_settings.music_directory.clone()];
                        _ = tree.insert(SETTINGS_KEY, serde_json::to_vec(&settings).expect("failed to serialize settings"));
                        log::info!(
                            "Migrated legacy music_directory '{}' to music_directories",
                            settings.metadata_settings.music_directory
                        );
                    }
                    settings
                }
                Err(e) => {
                    log::error!("Failed to deserialize settings from DB: {e}. Falling back to first-launch defaults.");
                    _ = tree.insert(
                        SETTINGS_KEY,
                        serde_json::to_vec(&first_launch_settings).expect("failed to serialize settings"),
                    );
                    first_launch_settings
                }
            }
        } else {
            _ = tree.insert(
                SETTINGS_KEY,
                serde_json::to_vec(&first_launch_settings).expect("failed to serialize settings"),
            );
            first_launch_settings
        };
        Arc::new(Self {
            tree,
            settings: RwLock::new(settings),
        })
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.read().expect("settings lock poisoned").clone()
    }

    pub fn get_settings_mut(&self) -> std::sync::RwLockWriteGuard<'_, Settings> {
        self.settings.write().expect("settings lock poisoned")
    }

    pub fn save_settings(&self, settings: &Settings) {
        *self.settings.write().expect("settings lock poisoned") = settings.clone();
        _ = self
            .tree
            .insert(SETTINGS_KEY, serde_json::to_vec(settings).expect("failed to serialize settings"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_models::common::VolumeCrtlType;

    fn open_db(path: &std::path::Path) -> Database {
        Database::builder(path.join("test.db")).open().expect("open temp db")
    }

    fn first_launch_settings() -> Settings {
        let mut settings = Settings::default();
        settings.volume_ctrl_settings.ctrl_device = VolumeCrtlType::Software;
        settings.volume_ctrl_settings.saved_volume = Some(50);
        settings.volume_ctrl_settings.volume_step = 5;
        settings
    }

    /// What a settings value looks like after a storage round trip —
    /// `skip_serializing` fields (e.g. `supported_extensions`) are dropped.
    fn persisted(settings: &Settings) -> Settings {
        serde_json::from_slice(&serde_json::to_vec(settings).expect("serialize")).expect("deserialize")
    }

    #[test]
    fn first_open_persists_first_launch_settings() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let db = open_db(tmp.path());
        let config = Configuration::new(&db, first_launch_settings());
        assert_eq!(config.get_settings(), first_launch_settings());

        // The defaults must be persisted, not just cached: a reopen with
        // different first-launch settings has to return the stored ones.
        drop(config);
        drop(db);
        let db = open_db(tmp.path());
        let config = Configuration::new(&db, Settings::default());
        assert_eq!(config.get_settings(), persisted(&first_launch_settings()));
    }

    #[test]
    fn stored_settings_win_over_first_launch_settings() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let db = open_db(tmp.path());
        let mut stored = Settings::default();
        stored.volume_ctrl_settings.saved_volume = Some(77);
        Configuration::new(&db, Settings::default()).save_settings(&stored);
        drop(db);

        let db = open_db(tmp.path());
        let config = Configuration::new(&db, first_launch_settings());
        assert_eq!(config.get_settings(), persisted(&stored));
    }

    #[test]
    fn unreadable_stored_settings_fall_back_to_persisted_first_launch_settings() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let db = open_db(tmp.path());
        let tree = db
            .keyspace("configuration", KeyspaceCreateOptions::default)
            .expect("open keyspace");
        tree.insert(SETTINGS_KEY, b"not json").expect("insert corrupt value");

        let config = Configuration::new(&db, first_launch_settings());
        assert_eq!(config.get_settings(), first_launch_settings());
        let stored = tree.get(SETTINGS_KEY).expect("read back").expect("value present");
        let stored: Settings = serde_json::from_slice(&stored).expect("valid json persisted");
        assert_eq!(stored, persisted(&first_launch_settings()));
    }
}
