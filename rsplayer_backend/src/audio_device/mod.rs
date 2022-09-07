use api_models::common::Volume;

pub(crate) mod ak4497;
pub(crate) mod alsa;
pub(crate) mod audio_service;

pub trait VolumeControlDevice {
    fn vol_up(&self) -> Volume;
    fn vol_down(&self) -> Volume;
    fn get_vol(&self) -> Volume;
    fn set_vol(&self, level: i64) -> Volume;
}
