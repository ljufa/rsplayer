//! Privileged SMB/NFS mount helper for the `RSPlayer` desktop app (Linux).
//!
//! The desktop app runs as an unprivileged user-session process, so it cannot
//! call `mount(2)` itself (that needs `CAP_SYS_ADMIN`). Instead it invokes
//! this helper through `pkexec`; the polkit action
//! `io.github.ljufa.rsplayer.mount-helper` (shipped in the desktop deb/rpm/arch
//! packages, `allow_active=yes`) authorizes users with an active local session
//! without a password prompt.
//!
//! The privileged surface is kept deliberately narrow:
//! - mounts/unmounts only under `/mnt/rsplayer/<name>` with a validated name
//! - filesystem type restricted to cifs / nfs4 / nfs
//! - mount options validated against a key whitelist
//! - the options string (it carries the SMB password) is read from stdin so it
//!   never shows up in `ps` or process command lines
//!
//! Usage:
//!   rsplayer-mount-helper mount <name> <smb|nfs> <source>   (options on stdin)
//!   rsplayer-mount-helper umount <name>

use std::process::ExitCode;

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["mount", name, fstype, source] => linux::mount_share(name, fstype, source),
        ["umount", name] => linux::unmount_share(name),
        _ => Err(
            "usage: rsplayer-mount-helper mount <name> <smb|nfs> <source> (options on stdin) | umount <name>"
                .to_string(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("rsplayer-mount-helper is only supported on Linux");
    ExitCode::FAILURE
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io::Read;

    use nix::mount::{mount, umount2, MntFlags, MsFlags};

    const MOUNT_BASE: &str = "/mnt/rsplayer";
    const MAX_NAME_LEN: usize = 64;
    const MAX_SOURCE_LEN: usize = 512;
    const MAX_OPTIONS_LEN: usize = 1024;

    /// Every option key the desktop backend may legitimately send
    /// (see `mount_service_linux.rs`); anything else is rejected.
    const ALLOWED_OPTION_KEYS: &[&str] = &[
        "username", "password", "domain", "uid", "gid", "file_mode", "dir_mode", "soft", "sec", "user", "pass",
        "vers", "addr", "nolock", "timeo", "retrans",
    ];

    pub fn mount_share(name: &str, fstype: &str, source: &str) -> Result<(), String> {
        let mount_point = validated_mount_point(name)?;
        validate_source(fstype, source)?;
        let options = read_options_from_stdin()?;

        fs::create_dir_all(&mount_point).map_err(|e| format!("Failed to create mount point {mount_point}: {e}"))?;

        match fstype {
            "smb" => mount_syscall(source, &mount_point, "cifs", &options),
            "nfs" => mount_syscall(source, &mount_point, "nfs4", &options)
                .or_else(|_| mount_syscall(source, &mount_point, "nfs", &options)),
            _ => Err(format!("Unsupported filesystem type: {fstype}")),
        }
    }

    pub fn unmount_share(name: &str) -> Result<(), String> {
        let mount_point = validated_mount_point(name)?;
        umount2(mount_point.as_str(), MntFlags::UMOUNT_NOFOLLOW).map_err(|e| format!("Unmount failed: {e}"))
    }

    fn mount_syscall(source: &str, target: &str, kernel_fstype: &str, options: &str) -> Result<(), String> {
        mount(
            Some(source),
            target,
            Some(kernel_fstype),
            MsFlags::empty(),
            Some(options),
        )
        .map_err(|e| format!("{kernel_fstype} mount failed: {e}"))
    }

    fn validated_mount_point(name: &str) -> Result<String, String> {
        let chars_ok = name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '));
        if name.is_empty() || name.len() > MAX_NAME_LEN || !chars_ok || name.starts_with('.') || name.starts_with('-')
        {
            return Err(format!("Invalid share name: {name}"));
        }
        Ok(format!("{MOUNT_BASE}/{name}"))
    }

    fn validate_source(fstype: &str, source: &str) -> Result<(), String> {
        let shape_ok = match fstype {
            "smb" => source.starts_with("//") && source.trim_start_matches('/').contains('/'),
            "nfs" => source.contains(':'),
            _ => false,
        };
        if !shape_ok || source.len() > MAX_SOURCE_LEN || source.chars().any(|c| c.is_control() || c == ',') {
            return Err(format!("Invalid mount source: {source}"));
        }
        Ok(())
    }

    fn read_options_from_stdin() -> Result<String, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("Failed to read mount options from stdin: {e}"))?;
        let options = input.lines().next().unwrap_or("").trim().to_string();
        validate_options(&options)?;
        Ok(options)
    }

    fn validate_options(options: &str) -> Result<(), String> {
        if options.is_empty() || options.len() > MAX_OPTIONS_LEN {
            return Err("Mount options are empty or too long".to_string());
        }
        for part in options.split(',') {
            let key = part.split('=').next().unwrap_or(part);
            if !ALLOWED_OPTION_KEYS.contains(&key) {
                // Deliberately not echoing the offending entry: a password
                // containing a comma would land here as a fragment.
                return Err("Mount options contain a disallowed or malformed entry".to_string());
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn accepts_valid_share_names() {
            assert_eq!(
                validated_mount_point("rpi5music").as_deref(),
                Ok("/mnt/rsplayer/rpi5music")
            );
            assert!(validated_mount_point("My NAS_2.0").is_ok());
        }

        #[test]
        fn rejects_traversal_and_bad_names() {
            assert!(validated_mount_point("").is_err());
            assert!(validated_mount_point("..").is_err());
            assert!(validated_mount_point("a/b").is_err());
            assert!(validated_mount_point("../etc").is_err());
            assert!(validated_mount_point(".hidden").is_err());
            assert!(validated_mount_point("-rf").is_err());
            assert!(validated_mount_point(&"x".repeat(65)).is_err());
        }

        #[test]
        fn validates_source_shape() {
            assert!(validate_source("smb", "//192.168.0.111/rpi5music").is_ok());
            assert!(validate_source("nfs", "192.168.0.111:/export/music").is_ok());
            assert!(validate_source("smb", "192.168.0.111:/export").is_err());
            assert!(validate_source("smb", "//server/share,evil").is_err());
            assert!(validate_source("proc", "proc").is_err());
        }

        #[test]
        fn whitelists_option_keys() {
            assert!(validate_options("username=music,password=music,domain=WORKGROUP,uid=1000,gid=1000,file_mode=0644,dir_mode=0755,soft").is_ok());
            assert!(validate_options("user=,pass=,sec=none,uid=1000,gid=1000,file_mode=0644,dir_mode=0755,soft").is_ok());
            assert!(validate_options("addr=192.168.0.111,nolock,soft,timeo=50,retrans=2").is_ok());
            assert!(validate_options("username=x,setuid").is_err());
            assert!(validate_options("").is_err());
        }
    }
}
