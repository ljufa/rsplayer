use std::fs;

use api_models::settings::{MetadataStoreSettings, NetworkMountConfig, NetworkMountType, NetworkStorageSettings};
use api_models::state::{ExternalMount, MountStatus, MusicDirStatus};
use log::{info, warn};
use nix::mount::{mount, umount, MsFlags};
use nix::unistd::{Gid, Uid};

const MOUNT_BASE: &str = "/mnt/rsplayer";
const CREDS_BASE: &str = "/opt/rsplayer";

pub fn mount_share(config: &NetworkMountConfig) -> Result<String, String> {
    let mount_point = config
        .mount_point
        .clone()
        .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", config.name));

    if config.mount_point.is_none() {
        // Create mount point directory
        fs::create_dir_all(&mount_point).map_err(|e| format!("Failed to create mount point: {e}"))?;
    }

    // If already mounted, unmount first
    if is_mounted(&mount_point) {
        let _ = unmount_share(&config.name, config.mount_point.as_deref());
    }

    match config.mount_type {
        NetworkMountType::Smb => mount_smb(config, &mount_point),
        NetworkMountType::Nfs => mount_nfs(config, &mount_point),
    }?;

    Ok(mount_point)
}

fn mount_smb(config: &NetworkMountConfig, mount_point: &str) -> Result<(), String> {
    let source = format!("//{}/{}", config.server, config.share);
    let uid = Uid::current();
    let gid = Gid::current();
    let creds_path = format!("{CREDS_BASE}/creds_{}", config.name);

    // Persist credentials to file so auto-mount on restart works without stored password
    if let Some(username) = config.username.as_deref().filter(|u| !u.is_empty()) {
        let password = config.password.as_deref().unwrap_or("");
        let creds_content = format!("username={username}\npassword={password}\n");
        fs::create_dir_all(CREDS_BASE).map_err(|e| format!("Failed to create creds dir: {e}"))?;
        fs::write(&creds_path, &creds_content).map_err(|e| format!("Failed to write creds file: {e}"))?;
    }

    let options = if std::path::Path::new(&creds_path).exists() {
        format!("credentials={creds_path},uid={uid},gid={gid},file_mode=0644,dir_mode=0755")
    } else {
        format!("user=,pass=,sec=none,uid={uid},gid={gid},file_mode=0644,dir_mode=0755")
    };
    info!("Mount: {source} with options: {options}");
    let result = mount(
        Some(source.as_str()),
        mount_point,
        Some("cifs"),
        MsFlags::empty(),
        Some(options.as_str()),
    );

    match result {
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

    // Try nfs4 syscall first
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

    // Try nfs syscall
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
    // Remove credentials file if it exists
    let creds_path = format!("{CREDS_BASE}/creds_{name}");
    let _ = fs::remove_file(&creds_path);
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
            let mounted = is_mounted(&mount_point);
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
        // Skip external mounts — they are managed by the system
        if mount_config.mount_point.is_some() {
            info!("Skipping external mount: {}", mount_config.name);
            continue;
        }
        info!("Auto-mounting network share: {}", mount_config.name);
        let mut last_err = String::new();
        let mut mounted = false;
        for attempt in 1..=3 {
            match mount_share(mount_config) {
                Ok(mp) => {
                    info!("Auto-mounted {} at {mp}", mount_config.name);
                    mounted = true;
                    break;
                }
                Err(e) => {
                    last_err = e;
                    if attempt < 3 {
                        warn!("Auto-mount attempt {attempt} failed for {}: {last_err}. Retrying in 5s...", mount_config.name);
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                }
            }
        }
        if !mounted {
            warn!("Failed to auto-mount {} after 3 attempts: {last_err}", mount_config.name);
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

    // Parse /proc/mounts for currently mounted network filesystems
    if let Ok(content) = fs::read_to_string("/proc/mounts") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let source = parts[0];
            let mount_point = parts[1];
            let fs_type = parts[2];

            // Only network filesystem types
            if !matches!(fs_type, "cifs" | "nfs" | "nfs4") {
                continue;
            }

            // Exclude already-saved mounts
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

    // Also parse /etc/fstab for configured-but-not-mounted network shares
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
            // Skip if already found in /proc/mounts
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
        // CIFS source format: //server/share or //server/share/path
        let stripped = ext.source.strip_prefix("//")?;
        let slash_pos = stripped.find('/')?;
        let server = stripped[..slash_pos].to_string();
        let share = stripped[slash_pos + 1..].to_string();
        (NetworkMountType::Smb, server, share)
    } else {
        // NFS source format: server:/path
        let colon_pos = ext.source.find(':')?;
        let server = ext.source[..colon_pos].to_string();
        let share = ext.source[colon_pos + 1..].to_string();
        (NetworkMountType::Nfs, server, share)
    };

    // Derive name from last path component of mount_point
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
