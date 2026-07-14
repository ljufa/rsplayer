//! Platform and packaging detection, plus the first-launch playback defaults
//! derived from it.
//!
//! [`PlatformProfile::detect`] probes the environment once at startup; the
//! pure [`PlatformProfile::first_launch_settings`] maps a profile to a
//! [`Settings`] value whose output device and volume control work out of the
//! box on that platform, so playback works before the user ever opens
//! Settings. The chosen device entries mirror what the settings UI offers:
//! the virtual "pipewire" card injected by `audio_device::alsa::get_all_cards`
//! on ALSA builds, and the "System Default" card the server synthesizes for
//! cpal-only builds (Windows/macOS).

use api_models::common::{PcmOutputDevice, VolumeCrtlType};
use api_models::settings::{InstallMethod, Settings};

use crate::audio_device::pipewire;

/// First-launch level for the `Software` gain control — silent at 0 by
/// default, so a moderate value is required for audible first playback.
const DEFAULT_SOFTWARE_VOLUME: u8 = 50;
const DEFAULT_VOLUME_STEP: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    MacOs,
    Linux,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sandbox {
    Flatpak,
    Snap,
    None,
}

/// Everything about the runtime environment that influences the playback
/// defaults, captured as plain data so the mapping to [`Settings`] stays a
/// pure, unit-testable function.
#[derive(Debug, Clone, Copy)]
pub struct PlatformProfile {
    pub os: TargetOs,
    pub sandbox: Sandbox,
    /// Whether this build ships the ALSA backend (Linux server/desktop builds
    /// do; Windows/macOS desktop builds enumerate devices through cpal only).
    pub alsa_backend: bool,
    /// `wpctl` or `pactl` is available to control the default sink volume.
    pub pipewire_ctl_available: bool,
    /// Playback can reach `PipeWire` — mirrors the condition under which the
    /// virtual "pipewire" card is offered in the device list.
    pub pipewire_playback_available: bool,
}

impl PlatformProfile {
    /// Probe the runtime environment. Called once at startup.
    pub fn detect() -> Self {
        let os = if cfg!(target_os = "windows") {
            TargetOs::Windows
        } else if cfg!(target_os = "macos") {
            TargetOs::MacOs
        } else if cfg!(target_os = "linux") {
            TargetOs::Linux
        } else {
            TargetOs::Other
        };
        Self {
            os,
            sandbox: detect_sandbox(),
            alsa_backend: cfg!(feature = "alsa"),
            pipewire_ctl_available: pipewire::is_volume_ctl_available(),
            pipewire_playback_available: pipewire::is_wpctl_available() || is_sandboxed_pipewire_available(),
        }
    }

    /// The settings persisted on first launch (or when the stored settings
    /// can't be read): platform-appropriate output device, volume-control
    /// type, and initial volume.
    #[must_use]
    pub fn first_launch_settings(&self) -> Settings {
        let mut settings = Settings::default();
        let volume = &mut settings.volume_ctrl_settings;
        volume.volume_step = DEFAULT_VOLUME_STEP;

        if self.alsa_backend && self.os == TargetOs::Linux && self.pipewire_playback_available {
            // Desktop Linux (flatpak, snap, or a host with WirePlumber):
            // play through PipeWire and control its default sink directly
            // when a CLI for that exists. No saved volume for the Pipewire
            // control — adopt the current system volume instead of mutating it.
            settings.alsa_settings.output_device = PcmOutputDevice {
                name: "default".to_string(),
                description: "Default Pipewire Device".to_string(),
                card_id: "pipewire".to_string(),
            };
            if self.pipewire_ctl_available {
                volume.ctrl_device = VolumeCrtlType::Pipewire;
                volume.saved_volume = None;
            } else {
                volume.ctrl_device = VolumeCrtlType::Software;
                volume.saved_volume = Some(DEFAULT_SOFTWARE_VOLUME);
            }
        } else if self.alsa_backend {
            // Headless Linux (deb/rpm systemd service, no user PipeWire
            // session): keep the empty device name, which resolves to the
            // cpal/ALSA default output. Software gain instead of an ALSA
            // mixer — guessing a mixer is unreliable and a mixerless
            // `AlsaMixer` silently does nothing.
            volume.ctrl_device = VolumeCrtlType::Software;
            volume.saved_volume = Some(DEFAULT_SOFTWARE_VOLUME);
        } else {
            // cpal-only builds (Windows WASAPI, macOS CoreAudio): the OS
            // default output device — never an ASIO driver — with software
            // gain, since there is no portable system-mixer control.
            settings.alsa_settings.output_device = PcmOutputDevice {
                name: "default".to_string(),
                description: "System Default".to_string(),
                card_id: "default".to_string(),
            };
            volume.ctrl_device = VolumeCrtlType::Software;
            volume.saved_volume = Some(DEFAULT_SOFTWARE_VOLUME);
        }
        settings
    }
}

/// Flatpak is detected via `/.flatpak-info`, Snap via the `SNAP_NAME` env var.
pub fn detect_sandbox() -> Sandbox {
    if std::path::Path::new("/.flatpak-info").exists() {
        Sandbox::Flatpak
    } else if std::env::var_os("SNAP_NAME").is_some() {
        Sandbox::Snap
    } else {
        Sandbox::None
    }
}

/// True when running inside a Flatpak or Snap sandbox.
pub fn is_sandboxed() -> bool {
    detect_sandbox() != Sandbox::None
}

/// Detect how this instance was installed, for the UI's update-command hint.
/// Cached — the answer cannot change while the process runs.
pub fn detect_install_method() -> InstallMethod {
    static METHOD: std::sync::OnceLock<InstallMethod> = std::sync::OnceLock::new();
    *METHOD.get_or_init(|| {
        // Flatpak/Snap take precedence over desktop mode: the sandboxed
        // desktop app sets RSPLAYER_DESKTOP too, but its updates arrive
        // through the sandbox store, not the releases page.
        match detect_sandbox() {
            Sandbox::Flatpak => InstallMethod::Flatpak,
            Sandbox::Snap => InstallMethod::Snap,
            Sandbox::None => {
                if std::env::var("RSPLAYER_DESKTOP").is_ok() {
                    InstallMethod::Desktop
                } else {
                    system_package_method().unwrap_or(InstallMethod::Unknown)
                }
            }
        }
    })
}

/// `Rpm`/`Deb` when the running binary sits at /usr/bin/rsplayer (only the
/// system packages install it there — manual builds run from elsewhere),
/// with os-release deciding the package family. `None` otherwise.
fn system_package_method() -> Option<InstallMethod> {
    let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    if exe != std::path::Path::new("/usr/bin/rsplayer") {
        return None;
    }
    let os_release = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .ok()?;
    let ids: Vec<String> = os_release
        .lines()
        .filter_map(|line| line.strip_prefix("ID=").or_else(|| line.strip_prefix("ID_LIKE=")))
        .flat_map(|value| value.trim_matches('"').split_whitespace())
        .map(str::to_lowercase)
        .collect();
    if ids.iter().any(|id| id == "debian" || id == "ubuntu") {
        Some(InstallMethod::Deb)
    } else if ids
        .iter()
        .any(|id| matches!(id.as_str(), "fedora" | "rhel" | "centos" | "suse" | "opensuse"))
    {
        Some(InstallMethod::Rpm)
    } else {
        None
    }
}

/// True when the sandbox exposes a `PulseAudio` compatibility socket.
///
/// Sandboxed runtimes (Flatpak, Snap) route the `default` ALSA PCM to the
/// host's `PipeWire` through that socket — playback works, so the virtual
/// Pipewire card should still be offered. Snapd remaps `XDG_RUNTIME_DIR` to
/// `/run/user/<uid>/snap.<name>`; the shared pulse socket lives one
/// directory up.
pub fn is_sandboxed_pipewire_available() -> bool {
    is_sandboxed()
        && std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| {
            let dir = std::path::Path::new(&dir);
            dir.join("pulse/native").exists() || dir.join("../pulse/native").exists()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(os: TargetOs, sandbox: Sandbox, alsa: bool, pw_ctl: bool, pw_playback: bool) -> PlatformProfile {
        PlatformProfile {
            os,
            sandbox,
            alsa_backend: alsa,
            pipewire_ctl_available: pw_ctl,
            pipewire_playback_available: pw_playback,
        }
    }

    fn assert_software_50(settings: &Settings) {
        assert_eq!(settings.volume_ctrl_settings.ctrl_device, VolumeCrtlType::Software);
        assert_eq!(settings.volume_ctrl_settings.saved_volume, Some(DEFAULT_SOFTWARE_VOLUME));
    }

    #[test]
    fn windows_defaults_to_system_default_device_and_software_gain() {
        let s = profile(TargetOs::Windows, Sandbox::None, false, false, false).first_launch_settings();
        assert_eq!(s.alsa_settings.output_device.name, "default");
        assert_eq!(s.alsa_settings.output_device.card_id, "default");
        assert_software_50(&s);
        assert_eq!(s.volume_ctrl_settings.volume_step, DEFAULT_VOLUME_STEP);
    }

    #[test]
    fn macos_defaults_to_system_default_device_and_software_gain() {
        let s = profile(TargetOs::MacOs, Sandbox::None, false, false, false).first_launch_settings();
        assert_eq!(s.alsa_settings.output_device.name, "default");
        assert_eq!(s.alsa_settings.output_device.card_id, "default");
        assert_software_50(&s);
    }

    #[test]
    fn sandboxed_linux_with_ctl_defaults_to_pipewire_adopting_system_volume() {
        for sandbox in [Sandbox::Flatpak, Sandbox::Snap] {
            let s = profile(TargetOs::Linux, sandbox, true, true, true).first_launch_settings();
            assert_eq!(s.alsa_settings.output_device.name, "default");
            assert_eq!(s.alsa_settings.output_device.card_id, "pipewire");
            assert_eq!(s.volume_ctrl_settings.ctrl_device, VolumeCrtlType::Pipewire);
            assert_eq!(s.volume_ctrl_settings.saved_volume, None);
        }
    }

    #[test]
    fn sandboxed_linux_without_ctl_falls_back_to_software_gain() {
        let s = profile(TargetOs::Linux, Sandbox::Snap, true, false, true).first_launch_settings();
        assert_eq!(s.alsa_settings.output_device.card_id, "pipewire");
        assert_software_50(&s);
    }

    #[test]
    fn linux_desktop_with_wireplumber_defaults_to_pipewire() {
        let s = profile(TargetOs::Linux, Sandbox::None, true, true, true).first_launch_settings();
        assert_eq!(s.alsa_settings.output_device.card_id, "pipewire");
        assert_eq!(s.volume_ctrl_settings.ctrl_device, VolumeCrtlType::Pipewire);
        assert_eq!(s.volume_ctrl_settings.saved_volume, None);
    }

    #[test]
    fn headless_linux_keeps_default_alsa_device_with_software_gain() {
        let s = profile(TargetOs::Linux, Sandbox::None, true, false, false).first_launch_settings();
        assert_eq!(s.alsa_settings.output_device.name, "");
        assert_software_50(&s);
    }
}
