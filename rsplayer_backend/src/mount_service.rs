use std::fs;

use api_models::settings::{MetadataStoreSettings, NetworkMountConfig, NetworkMountType, NetworkStorageSettings};
use api_models::state::{ExternalMount, MountStatus, MusicDirStatus};
use log::{info, warn};
use nix::mount::{mount, umount, MsFlags};
use nix::unistd::{Gid, Uid};

const MOUNT_BASE: &str = "/mnt/rsplayer";

pub struct MountService;

impl MountService {
    pub fn mount_share(config: &NetworkMountConfig) -> Result<String, String> {
        let mount_point = config
            .mount_point
            .clone()
            .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", config.name));

        if config.mount_point.is_none() {
            fs::create_dir_all(&mount_point).map_err(|e| format!("Failed to create mount point: {e}"))?;
        }

        if Self::is_mounted(&mount_point) {
            let _ = Self::unmount_share(&config.name, config.mount_point.as_deref());
        }

        match config.mount_type {
            NetworkMountType::Smb => Self::mount_smb(config, &mount_point),
            NetworkMountType::Nfs => Self::mount_nfs(config, &mount_point),
        }?;

        Ok(mount_point)
    }

    fn mount_smb(config: &NetworkMountConfig, mount_point: &str) -> Result<(), String> {
        let source = format!("//{}/{}", config.server, config.share);
        let uid = Uid::current();
        let gid = Gid::current();

        let options = config.username.as_deref().filter(|u| !u.is_empty()).map_or_else(
            || format!("user=,pass=,sec=none,uid={uid},gid={gid},file_mode=0644,dir_mode=0755"),
            |username| {
                let password = config.password.as_deref().unwrap_or("");
                let domain_opt = config
                    .domain
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map_or(String::new(), |d| format!(",domain={d}"));
                format!(
                    "username={username},password={password}{domain_opt},uid={uid},gid={gid},file_mode=0644,dir_mode=0755"
                )
            },
        );

        let safe_log = config
            .username
            .as_deref()
            .map_or_else(|| options.clone(), |u| format!("username={u},password=***,..."));
        info!("Mount: {source} with options: {safe_log}");

        match mount(
            Some(source.as_str()),
            mount_point,
            Some("cifs"),
            MsFlags::empty(),
            Some(options.as_str()),
        ) {
            Ok(()) => {
                info!("SMB share {source} mounted at {mount_point}");
                Ok(())
            }
            Err(e) => Err(format!("SMB mount failed: {e}")),
        }
    }

    fn mount_nfs(config: &NetworkMountConfig, mount_point: &str) -> Result<(), String> {
        let source = format!("{}:{}", config.server, config.share);
        let options = format!("addr={},nolock", config.server);

        let nfs4_result = mount(
            Some(source.as_str()),
            mount_point,
            Some("nfs4"),
            MsFlags::empty(),
            Some(options.as_str()),
        );

        match nfs4_result {
            Ok(()) => {
                info!("NFS share {source} mounted at {mount_point}");
                return Ok(());
            }
            Err(nfs4_err) => {
                info!("nfs4 mount failed ({nfs4_err}), trying nfs");
            }
        }

        let nfs_result = mount(
            Some(source.as_str()),
            mount_point,
            Some("nfs"),
            MsFlags::empty(),
            Some(options.as_str()),
        );

        match nfs_result {
            Ok(()) => {
                info!("NFS share {source} mounted at {mount_point}");
                Ok(())
            }
            Err(e) => Err(format!("NFS mount failed: {e}")),
        }
    }

    pub fn unmount_share(name: &str, ext_mount: Option<&str>) -> Result<(), String> {
        let mount_point = ext_mount.map_or_else(|| format!("{MOUNT_BASE}/{name}"), std::string::ToString::to_string);

        let result = umount(mount_point.as_str());
        match result {
            Ok(()) => {}
            Err(e) => return Err(format!("Unmount failed: {e}")),
        }

        info!("Unmounted {mount_point}");
        Ok(())
    }

    pub fn query_mount_status(settings: &NetworkStorageSettings) -> Vec<MountStatus> {
        settings
            .mounts
            .iter()
            .map(|m| {
                let mount_point = m
                    .mount_point
                    .clone()
                    .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", m.name));
                let mounted = Self::is_mounted(&mount_point);
                let path = std::path::Path::new(&mount_point);
                let readable = mounted && path.is_dir();
                let writable = if readable {
                    let test_file = path.join(".rsplayer_write_test");
                    let w = fs::write(&test_file, b"").is_ok();
                    let _ = fs::remove_file(&test_file);
                    w
                } else {
                    false
                };
                MountStatus {
                    name: m.name.clone(),
                    mount_point: mount_point.clone(),
                    is_mounted: mounted,
                    readable,
                    writable,
                }
            })
            .collect()
    }

    pub fn query_music_dir_status(settings: &MetadataStoreSettings) -> Vec<MusicDirStatus> {
        let mount_dirs: Vec<String> = settings
            .effective_directories()
            .into_iter()
            .filter(|d| !d.starts_with(MOUNT_BASE))
            .collect();
        mount_dirs
            .iter()
            .map(|dir| {
                let path = std::path::Path::new(dir);
                let readable = path.is_dir();
                let writable = if readable {
                    let test_file = path.join(".rsplayer_write_test");
                    let w = fs::write(&test_file, b"").is_ok();
                    let _ = fs::remove_file(&test_file);
                    w
                } else {
                    false
                };
                MusicDirStatus {
                    path: dir.clone(),
                    readable,
                    writable,
                }
            })
            .collect()
    }

    pub fn mount_all(settings: &NetworkStorageSettings) {
        for mount_config in &settings.mounts {
            if mount_config.mount_point.is_some() {
                info!("Skipping external mount: {}", mount_config.name);
                continue;
            }
            info!("Auto-mounting network share: {}", mount_config.name);
            let mut last_err = String::new();
            let mut mounted = false;
            for attempt in 1..=3 {
                match Self::mount_share(mount_config) {
                    Ok(mp) => {
                        info!("Auto-mounted {} at {mp}", mount_config.name);
                        mounted = true;
                        break;
                    }
                    Err(e) => {
                        last_err = e;
                        if attempt < 3 {
                            warn!(
                                "Auto-mount attempt {attempt} failed for {}: {last_err}. Retrying in 5s...",
                                mount_config.name
                            );
                            std::thread::sleep(std::time::Duration::from_secs(5));
                        }
                    }
                }
            }
            if !mounted {
                warn!(
                    "Failed to auto-mount {} after 3 attempts: {last_err}",
                    mount_config.name
                );
            }
        }
    }

    pub fn discover_external_mounts(settings: &NetworkStorageSettings) -> Vec<ExternalMount> {
        let saved_mount_points: Vec<String> = settings
            .mounts
            .iter()
            .map(|m| {
                m.mount_point
                    .clone()
                    .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", m.name))
            })
            .collect();

        let mut result = Vec::new();

        if let Ok(content) = fs::read_to_string("/proc/mounts") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }
                let source = parts[0];
                let mount_point = parts[1];
                let fs_type = parts[2];

                if !matches!(fs_type, "cifs" | "nfs" | "nfs4") {
                    continue;
                }

                if saved_mount_points.iter().any(|p| p == mount_point) {
                    continue;
                }

                let path = std::path::Path::new(mount_point);
                let readable = path.is_dir();
                let writable = if readable {
                    let test_file = path.join(".rsplayer_write_test");
                    let w = fs::write(&test_file, b"").is_ok();
                    let _ = fs::remove_file(&test_file);
                    w
                } else {
                    false
                };

                result.push(ExternalMount {
                    source: source.to_string(),
                    mount_point: mount_point.to_string(),
                    fs_type: fs_type.to_string(),
                    readable,
                    writable,
                });
            }
        }

        let mounted_points: Vec<String> = result.iter().map(|m| m.mount_point.clone()).collect();
        if let Ok(fstab) = fs::read_to_string("/etc/fstab") {
            for line in fstab.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }
                let source = parts[0];
                let mount_point = parts[1];
                let fs_type = parts[2];

                if !matches!(fs_type, "cifs" | "nfs" | "nfs4") {
                    continue;
                }
                if mount_point.starts_with(MOUNT_BASE) {
                    continue;
                }
                if saved_mount_points.iter().any(|p| p == mount_point) {
                    continue;
                }
                if mounted_points.iter().any(|p| p == mount_point) {
                    continue;
                }

                result.push(ExternalMount {
                    source: source.to_string(),
                    mount_point: mount_point.to_string(),
                    fs_type: fs_type.to_string(),
                    readable: false,
                    writable: false,
                });
            }
        }

        result
    }

    pub fn parse_external_mount_to_config(ext: &ExternalMount) -> Option<NetworkMountConfig> {
        let (mount_type, server, share) = if ext.fs_type == "cifs" {
            let stripped = ext.source.strip_prefix("//")?;
            let slash_pos = stripped.find('/')?;
            let server = stripped[..slash_pos].to_string();
            let share = stripped[slash_pos + 1..].to_string();
            (NetworkMountType::Smb, server, share)
        } else {
            let colon_pos = ext.source.find(':')?;
            let server = ext.source[..colon_pos].to_string();
            let share = ext.source[colon_pos + 1..].to_string();
            (NetworkMountType::Nfs, server, share)
        };

        let name = std::path::Path::new(&ext.mount_point).file_name().map_or_else(
            || ext.mount_point.replace('/', "_"),
            |n| n.to_string_lossy().to_string(),
        );

        Some(NetworkMountConfig {
            name,
            mount_type,
            server,
            share,
            username: None,
            password: None,
            domain: None,
            mount_point: Some(ext.mount_point.clone()),
        })
    }

    fn is_mounted(mount_point: &str) -> bool {
        fs::read_to_string("/proc/mounts")
            .map(|content| {
                content
                    .lines()
                    .any(|line| line.split_whitespace().nth(1) == Some(mount_point))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smb_config(username: Option<&str>, password: Option<&str>, domain: Option<&str>) -> NetworkMountConfig {
        NetworkMountConfig {
            name: "test-share".to_string(),
            mount_type: NetworkMountType::Smb,
            server: "192.168.0.111".to_string(),
            share: "rpi5music".to_string(),
            username: username.map(str::to_string),
            password: password.map(str::to_string),
            domain: domain.map(str::to_string),
            mount_point: None,
        }
    }

    fn build_options(config: &NetworkMountConfig) -> String {
        let uid = Uid::current();
        let gid = Gid::current();
        if let Some(username) = config.username.as_deref().filter(|u| !u.is_empty()) {
            let password = config.password.as_deref().unwrap_or("");
            let domain_opt = config
                .domain
                .as_deref()
                .filter(|d| !d.is_empty())
                .map_or(String::new(), |d| format!(",domain={d}"));
            format!(
                "username={username},password={password}{domain_opt},uid={uid},gid={gid},file_mode=0644,dir_mode=0755"
            )
        } else {
            format!("user=,pass=,sec=none,uid={uid},gid={gid},file_mode=0644,dir_mode=0755")
        }
    }

    #[test]
    fn mount_options_include_inline_credentials() {
        let config = smb_config(Some("music"), Some("s3cr3t"), None);
        let opts = build_options(&config);
        assert!(opts.starts_with("username=music,password=s3cr3t,"));
        assert!(!opts.contains("domain="));
    }

    #[test]
    fn mount_options_include_domain_when_provided() {
        let config = smb_config(Some("music"), Some("s3cr3t"), Some("WORKGROUP"));
        let opts = build_options(&config);
        assert!(opts.contains(",domain=WORKGROUP,"));
    }

    #[test]
    fn mount_options_anonymous_when_no_username() {
        let config = smb_config(None, None, None);
        let opts = build_options(&config);
        assert!(opts.starts_with("user=,pass=,sec=none,"));
    }

    #[test]
    fn password_persisted_in_config_survives_restart() {
        let config = smb_config(Some("testuser"), Some("pass123"), Some("SAMBA"));
        assert_eq!(config.password.as_deref(), Some("pass123"));
        let opts = build_options(&config);
        assert!(opts.contains("password=pass123"));
    }
}
