//! Volume control abstraction.
//!
//! [`VolumeControlDevice`] has one implementation per
//! `VolumeCrtlType`: ALSA mixer (Linux, feature `alsa`), `PipeWire`
//! (`wpctl`), pure software gain (applied to PCM on the playback path),
//! the front-panel firmware's hardware attenuator, or no-op. Selected and
//! wired in `audio_service`.

use api_models::common::Volume;

#[cfg(feature = "alsa")]
pub mod alsa;
pub mod audio_service;
pub mod pipewire;
pub mod rsp_firmware;
pub mod software_gain;

pub trait VolumeControlDevice {
    fn vol_up(&mut self) -> Volume;
    fn vol_down(&mut self) -> Volume;
    fn get_vol(&mut self) -> Volume;
    fn set_vol(&mut self, level: u8) -> Volume;
}

pub struct NoOpVolumeControlDevice;
impl VolumeControlDevice for NoOpVolumeControlDevice {
    fn vol_up(&mut self) -> Volume {
        Volume::default()
    }
    fn vol_down(&mut self) -> Volume {
        Volume::default()
    }
    fn get_vol(&mut self) -> Volume {
        Volume::default()
    }
    fn set_vol(&mut self, _level: u8) -> Volume {
        Volume::default()
    }
}
