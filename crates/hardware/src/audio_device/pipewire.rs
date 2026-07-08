//! `PipeWire` volume control by shelling out to `wpctl` against
//! `@DEFAULT_AUDIO_SINK@`, falling back to `pactl` against `@DEFAULT_SINK@`
//! where `WirePlumber`'s CLI is unavailable (e.g. the Flatpak runtime, which
//! ships `pactl` and reaches the host's pipewire-pulse over the pulse socket).

use api_models::common::Volume;
use log::error;
use std::process::Command;

use super::VolumeControlDevice;

/// True when `WirePlumber`'s `wpctl` CLI is present.
pub fn is_wpctl_available() -> bool {
    Command::new("wpctl").arg("--version").output().is_ok()
}

fn is_pactl_available() -> bool {
    Command::new("pactl").arg("--version").output().is_ok()
}

/// True when some CLI capable of controlling the default sink volume exists.
/// Gates offering the `Pipewire` volume control type in settings.
pub fn is_volume_ctl_available() -> bool {
    is_wpctl_available() || is_pactl_available()
}

#[derive(Clone, Copy)]
enum Backend {
    Wpctl,
    Pactl,
}

pub struct PipewireVolumeControlDevice {
    current_vol: Volume,
    backend: Backend,
}

impl Default for PipewireVolumeControlDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl PipewireVolumeControlDevice {
    pub fn new() -> Self {
        let backend = if is_wpctl_available() { Backend::Wpctl } else { Backend::Pactl };
        let mut dev = Self {
            current_vol: Volume {
                current: 50,
                max: 100,
                min: 0,
                step: 5,
            },
            backend,
        };
        dev.get_vol();
        dev
    }
}

impl VolumeControlDevice for PipewireVolumeControlDevice {
    fn vol_up(&mut self) -> Volume {
        let ev = self.get_vol();
        if let Some(nv) = ev.current.checked_add(ev.step) {
            if nv <= ev.max {
                self.set_vol(nv);
            } else {
                self.set_vol(ev.max);
            }
        }
        self.get_vol()
    }

    fn vol_down(&mut self) -> Volume {
        let ev = self.get_vol();
        if ev.current > ev.step {
            self.set_vol(ev.current - ev.step);
        } else {
            self.set_vol(ev.min);
        }
        self.get_vol()
    }

    fn get_vol(&mut self) -> Volume {
        let output = match self.backend {
            Backend::Wpctl => Command::new("wpctl").arg("get-volume").arg("@DEFAULT_AUDIO_SINK@").output(),
            Backend::Pactl => Command::new("pactl").arg("get-sink-volume").arg("@DEFAULT_SINK@").output(),
        };

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                match self.backend {
                    Backend::Wpctl => {
                        // format is "Volume: 0.56"
                        if let Some(vol_str) = stdout.trim().split(": ").nth(1) {
                            // Extract just the number, ignoring [MUTED] etc
                            let num_str = vol_str.split_whitespace().next().unwrap_or(vol_str);
                            if let Ok(vol_float) = num_str.parse::<f32>() {
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                let vol_int = (vol_float * 100.0).round() as u8;
                                self.current_vol.current = vol_int;
                            }
                        }
                    }
                    Backend::Pactl => {
                        // format is "Volume: front-left: 39322 /  60% / -13.31 dB, ..."
                        if let Some(pct) = stdout.split_whitespace().find_map(|t| t.strip_suffix('%'))
                            && let Ok(vol_int) = pct.parse::<u8>()
                        {
                            self.current_vol.current = vol_int;
                        }
                    }
                }
            }
            _ => {
                error!("Failed to get volume from wpctl/pactl");
            }
        }
        self.current_vol
    }

    fn set_vol(&mut self, level: u8) -> Volume {
        let level = level.clamp(self.current_vol.min, self.current_vol.max);

        let output = match self.backend {
            Backend::Wpctl => {
                #[allow(clippy::cast_precision_loss)]
                let level_float = f32::from(level) / 100.0;
                Command::new("wpctl")
                    .arg("set-volume")
                    .arg("@DEFAULT_AUDIO_SINK@")
                    .arg(format!("{level_float:.2}"))
                    .output()
            }
            Backend::Pactl => Command::new("pactl")
                .arg("set-sink-volume")
                .arg("@DEFAULT_SINK@")
                .arg(format!("{level}%"))
                .output(),
        };

        if let Err(e) = output {
            error!("Failed to set volume via wpctl/pactl: {e}");
        } else if let Ok(o) = output
            && !o.status.success()
        {
            error!("wpctl/pactl returned error: {}", String::from_utf8_lossy(&o.stderr));
        }

        self.get_vol()
    }
}
