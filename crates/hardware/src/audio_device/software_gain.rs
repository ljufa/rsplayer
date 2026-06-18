use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use api_models::common::Volume;

use super::VolumeControlDevice;

pub const SOFTWARE_GAIN_MAX: u8 = 100;
pub const SOFTWARE_GAIN_STEP: u8 = 5;

pub struct SoftwareGainVolumeControlDevice {
    level: Arc<AtomicU8>,
}

impl SoftwareGainVolumeControlDevice {
    pub fn new(level: Arc<AtomicU8>) -> Box<Self> {
        Box::new(Self { level })
    }

    const fn snapshot(&self, current: u8) -> Volume {
        Volume {
            step: SOFTWARE_GAIN_STEP,
            min: 0,
            max: SOFTWARE_GAIN_MAX,
            current,
        }
    }
}

impl VolumeControlDevice for SoftwareGainVolumeControlDevice {
    fn vol_up(&mut self) -> Volume {
        let current = self.level.load(Ordering::Relaxed);
        let next = current.saturating_add(SOFTWARE_GAIN_STEP).min(SOFTWARE_GAIN_MAX);
        self.level.store(next, Ordering::Relaxed);
        self.snapshot(next)
    }

    fn vol_down(&mut self) -> Volume {
        let current = self.level.load(Ordering::Relaxed);
        let next = current.saturating_sub(SOFTWARE_GAIN_STEP);
        self.level.store(next, Ordering::Relaxed);
        self.snapshot(next)
    }

    fn get_vol(&mut self) -> Volume {
        self.snapshot(self.level.load(Ordering::Relaxed))
    }

    fn set_vol(&mut self, level: u8) -> Volume {
        let clamped = level.min(SOFTWARE_GAIN_MAX);
        self.level.store(clamped, Ordering::Relaxed);
        self.snapshot(clamped)
    }
}
