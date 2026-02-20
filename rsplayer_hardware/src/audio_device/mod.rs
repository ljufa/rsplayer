use api_models::common::Volume;

pub mod alsa;
pub mod audio_service;
pub mod rsp_firmware;
// pub mod test;

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
