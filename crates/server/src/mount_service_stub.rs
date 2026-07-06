//! No-op `MountService` for non-Linux platforms: mounting fails with a
//! clear message, status queries return local-directory information only.

use api_models::settings::{MetadataStoreSettings, NetworkMountConfig, NetworkStorageSettings};
use api_models::state::{ExternalMount, MountStatus, MusicDirStatus};

pub struct MountService;

impl MountService {
    pub fn mount_share(_config: &NetworkMountConfig) -> Result<String, String> {
        Err("Network share mounting is not supported on this platform".to_string())
    }

    pub fn unmount_share(_name: &str, _ext_mount: Option<&str>) -> Result<(), String> {
        Err("Network share mounting is not supported on this platform".to_string())
    }

    pub fn query_mount_status(_settings: &NetworkStorageSettings) -> Vec<MountStatus> {
        Vec::new()
    }

    pub fn query_music_dir_status(settings: &MetadataStoreSettings) -> Vec<MusicDirStatus> {
        settings
            .effective_directories()
            .into_iter()
            .map(|dir| {
                let path = std::path::Path::new(&dir);
                let readable = path.is_dir();
                let writable = if readable {
                    let test_file = path.join(".rsplayer_write_test");
                    let w = std::fs::write(&test_file, b"").is_ok();
                    let _ = std::fs::remove_file(&test_file);
                    w
                } else {
                    false
                };
                MusicDirStatus {
                    path: dir,
                    readable,
                    writable,
                }
            })
            .collect()
    }

    pub fn mount_all(_settings: &NetworkStorageSettings) {}

    pub fn discover_external_mounts(_settings: &NetworkStorageSettings) -> Vec<ExternalMount> {
        Vec::new()
    }

    pub fn parse_external_mount_to_config(_ext: &ExternalMount) -> Option<NetworkMountConfig> {
        None
    }
}
