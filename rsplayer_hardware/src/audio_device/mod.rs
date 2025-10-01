use api_models::common::Volume;

pub mod alsa;
pub mod audio_service;
// pub mod test;

pub trait VolumeControlDevice {
    fn vol_up(&self) -> Volume;
    fn vol_down(&self) -> Volume;
    fn get_vol(&self) -> Volume;
    fn set_vol(&self, level: u8) -> Volume;
}
