use api_models::common::Volume;
use std::process::Command;
use log::error;

use super::VolumeControlDevice;

pub struct PipewireVolumeControlDevice {
    current_vol: Volume,
}

impl PipewireVolumeControlDevice {
    pub fn new() -> Self {
        let mut dev = Self {
            current_vol: Volume {
                current: 50,
                max: 100,
                min: 0,
                step: 5,
            },
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
        let output = Command::new("wpctl")
            .arg("get-volume")
            .arg("@DEFAULT_AUDIO_SINK@")
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
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
            _ => {
                error!("Failed to get volume from wpctl");
            }
        }
        self.current_vol.clone()
    }

    fn set_vol(&mut self, level: u8) -> Volume {
        let level = level.clamp(self.current_vol.min, self.current_vol.max);
        
        #[allow(clippy::cast_precision_loss)]
        let level_float = (level as f32) / 100.0;
        
        let output = Command::new("wpctl")
            .arg("set-volume")
            .arg("@DEFAULT_AUDIO_SINK@")
            .arg(format!("{:.2}", level_float))
            .output();
            
        if let Err(e) = output {
            error!("Failed to set volume via wpctl: {}", e);
        } else if let Ok(o) = output {
            if !o.status.success() {
                 error!("wpctl returned error: {}", String::from_utf8_lossy(&o.stderr));
            }
        }
            
        self.get_vol()
    }
}
