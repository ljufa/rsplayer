//! SMB/NFS mounting of network music shares (Linux implementation; other
//! platforms get the no-op `mount_service_stub`).
//!
//! Mounts under `/mnt/rsplayer/<name>` via nix `mount(2)` — requires the
//! binary to run with the needed privileges — with reachability pre-checks
//! and read-only fallbacks. In desktop mode (`RSPLAYER_DESKTOP` set) the app
//! runs unprivileged, so mount/umount syscalls are delegated to the
//! `rsplayer-mount-helper` binary through `pkexec` (polkit action
//! `io.github.ljufa.rsplayer.mount-helper`, shipped by the desktop packages).

use std::fs;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::Duration;

use api_models::settings::{MetadataStoreSettings, NetworkMountConfig, NetworkMountType, NetworkStorageSettings};
use api_models::state::{ExternalMount, MountStatus, MusicDirStatus};
use log::{info, warn};
use nix::mount::{mount, umount, MsFlags};
use nix::unistd::{Gid, Uid};

const MOUNT_BASE: &str = "/mnt/rsplayer";
const MOUNT_HELPER_PATH: &str = "/usr/libexec/rsplayer-mount-helper";
const NFS_IO_TIMEOUT_DECISECONDS: u32 = 50;
const NFS_RETRIES: u32 = 2;
const SMB_PORT: u16 = 445;
const SMB_CONNECT_TIMEOUT_SECS: u64 = 3;

pub struct MountService;

impl MountService {
    pub fn mount_share(config: &NetworkMountConfig) -> Result<String, String> {
        let mount_point = config
            .mount_point
            .clone()
            .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", config.name));

        // In helper mode the mount point is created by the (root) helper —
        // an unprivileged desktop process cannot mkdir under /mnt.
        if config.mount_point.is_none() && !Self::use_privileged_helper() {
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

        Self::ensure_tcp_connectivity(&config.server, SMB_PORT, Duration::from_secs(SMB_CONNECT_TIMEOUT_SECS))?;

        let options = config.username.as_deref().filter(|u| !u.is_empty()).map_or_else(
            || format!("user=,pass=,sec=none,uid={uid},gid={gid},file_mode=0644,dir_mode=0755,soft"),
            |username| {
                let password = config.password.as_deref().unwrap_or("");
                let domain_opt = config
                    .domain
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map_or(String::new(), |d| format!(",domain={d}"));
                format!("username={username},password={password}{domain_opt},uid={uid},gid={gid},file_mode=0644,dir_mode=0755,soft")
            },
        );

        let safe_log = config
            .username
            .as_deref()
            .map_or_else(|| options.clone(), |u| format!("username={u},password=***,..."));
        info!("Mount: {source} with options: {safe_log}");

        if Self::helper_applies(config) {
            Self::helper_mount(&config.name, "smb", &source, &options)?;
            info!("SMB share {source} mounted at {mount_point} (via helper)");
            return Ok(());
        }

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

    fn ensure_tcp_connectivity(server: &str, port: u16, timeout: Duration) -> Result<(), String> {
        let addrs: Vec<_> = (server, port)
            .to_socket_addrs()
            .map_err(|e| format!("Failed to resolve {server}:{port}: {e}"))?
            .collect();

        if addrs.is_empty() {
            return Err(format!("No addresses resolved for {server}:{port}"));
        }

        let mut last_err = String::new();
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_err = format!("{addr}: {e}");
                }
            }
        }

        Err(format!(
            "SMB server {server}:{port} is unreachable (timeout {}s): {last_err}",
            timeout.as_secs()
        ))
    }

    fn mount_nfs(config: &NetworkMountConfig, mount_point: &str) -> Result<(), String> {
        let source = format!("{}:{}", config.server, config.share);
        let options = format!(
            "addr={},nolock,soft,timeo={},retrans={}",
            config.server, NFS_IO_TIMEOUT_DECISECONDS, NFS_RETRIES
        );

        info!("Mount: {source} with options: {options}");

        if Self::helper_applies(config) {
            // The helper does the nfs4 -> nfs fallback internally.
            Self::helper_mount(&config.name, "nfs", &source, &options)?;
            info!("NFS share {source} mounted at {mount_point} (via helper)");
            return Ok(());
        }

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

        // The helper only manages mounts at /mnt/rsplayer/<name>; external
        // mount points elsewhere go through the direct syscall.
        let helper_manageable = ext_mount.is_none_or(|mp| mp == format!("{MOUNT_BASE}/{name}"));
        if helper_manageable && Self::use_privileged_helper() {
            Self::helper_umount(name)?;
        } else if let Err(e) = umount(mount_point.as_str()) {
            return Err(format!("Unmount failed: {e}"));
        }

        info!("Unmounted {mount_point}");
        Ok(())
    }

    /// Whether mount/umount syscalls should be delegated to the privileged
    /// `pkexec` helper: only in desktop mode (unprivileged user-session
    /// process) and only when the desktop package installed the helper.
    /// Server installs keep the direct syscall path (systemd grants
    /// `CAP_SYS_ADMIN`), as do dev runs and sandboxed (flatpak/snap) builds
    /// where the helper is absent.
    fn use_privileged_helper() -> bool {
        std::env::var("RSPLAYER_DESKTOP").is_ok() && std::path::Path::new(MOUNT_HELPER_PATH).exists()
    }

    /// The helper only mounts at `/mnt/rsplayer/<name>`. That covers configs
    /// without an explicit mount point, but also saved "detected external
    /// mounts" whose mount point happens to be exactly `/mnt/rsplayer/<name>`
    /// (e.g. re-saved from a server instance's managed mounts). Anything
    /// pointing elsewhere keeps the direct syscall path.
    fn helper_applies(config: &NetworkMountConfig) -> bool {
        Self::use_privileged_helper()
            && config
                .mount_point
                .as_deref()
                .is_none_or(|mp| mp == format!("{MOUNT_BASE}/{}", config.name))
    }

    fn helper_mount(name: &str, fstype: &str, source: &str, options: &str) -> Result<(), String> {
        let mut child = Command::new("pkexec")
            .args([MOUNT_HELPER_PATH, "mount", name, fstype, source])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to run pkexec: {e}"))?;

        // Options carry the SMB password — passed on stdin so they never
        // appear in the process list.
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(options.as_bytes())
                .map_err(|e| format!("Failed to send mount options to helper: {e}"))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| format!("Mount helper did not finish: {e}"))?;
        Self::helper_result(&output, "Mount")
    }

    fn helper_umount(name: &str) -> Result<(), String> {
        let output = Command::new("pkexec")
            .args([MOUNT_HELPER_PATH, "umount", name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run pkexec: {e}"))?;
        Self::helper_result(&output, "Unmount")
    }

    fn helper_result(output: &std::process::Output, action: &str) -> Result<(), String> {
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // pkexec itself exits 126 when the auth dialog is dismissed and 127
        // when authorization is denied; anything else is the helper's error.
        match output.status.code() {
            Some(126) => Err(format!("{action} cancelled: authorization dialog was dismissed")),
            Some(127) => Err(format!("{action} not authorized by system policy: {stderr}")),
            _ => Err(if stderr.is_empty() {
                format!("{action} helper failed")
            } else {
                stderr
            }),
        }
    }

    pub fn query_mount_status(settings: &NetworkStorageSettings) -> Vec<MountStatus> {
        settings
            .mounts
            .iter()
            .map(|m| {
                let mount_point = m.mount_point.clone().unwrap_or_else(|| format!("{MOUNT_BASE}/{}", m.name));
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
            let mount_point = mount_config
                .mount_point
                .clone()
                .unwrap_or_else(|| format!("{MOUNT_BASE}/{}", mount_config.name));
            if Self::is_mounted(&mount_point) {
                info!("Skipping already-mounted share: {}", mount_config.name);
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
                warn!("Failed to auto-mount {} after 3 attempts: {last_err}", mount_config.name);
            }
        }
    }

    pub fn discover_external_mounts(settings: &NetworkStorageSettings) -> Vec<ExternalMount> {
        let saved_mount_points: Vec<String> = settings
            .mounts
            .iter()
            .map(|m| m.mount_point.clone().unwrap_or_else(|| format!("{MOUNT_BASE}/{}", m.name)))
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

        let name = std::path::Path::new(&ext.mount_point)
            .file_name()
            .map_or_else(|| ext.mount_point.replace('/', "_"), |n| n.to_string_lossy().to_string());

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
            .is_ok_and(|content| content.lines().any(|line| line.split_whitespace().nth(1) == Some(mount_point)))
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
            format!("username={username},password={password}{domain_opt},uid={uid},gid={gid},file_mode=0644,dir_mode=0755")
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
