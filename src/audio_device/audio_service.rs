use api_models::common::Volume;
use cfg_if::cfg_if;

use crate::common::{MutArcConfiguration, Result};

#[cfg(feature = "hw_dac")]
use super::ak4497::DacAk4497;
#[cfg(not(feature = "hw_dac"))]
use super::alsa::AlsaMixer;

use super::{alsa::AlsaPcmCard, VolumeControlDevice};

pub struct AudioInterfaceService {
    alsa_card: AlsaPcmCard,
    volume_ctrl_device: Box<dyn VolumeControlDevice>,
}

impl AudioInterfaceService {
    pub fn new(config: MutArcConfiguration) -> Result<Self> {
        let settings = config.lock().unwrap().get_settings();
        let ac = AlsaPcmCard::new(settings.alsa_settings.device_name.clone());
        // ac.wait_unlock_audio_dev()?;

        cfg_if! {
            if #[cfg(feature="hw_dac")] {
                let volume_ctrl_device = DacAk4497::new(
                    config.lock().unwrap().get_streamer_status().volume_state,
                    &settings.dac_settings,
                )?;
            } else if #[cfg(not(feature="hw_dac"))] {
                let volume_ctrl_device = AlsaMixer::new(
                    settings.alsa_settings.device_name,
                    "Master".to_string(),
                    0,
                )?;
            }
        }
        Ok(Self {
            alsa_card: ac,
            volume_ctrl_device,
        })
    }
    pub fn is_device_in_use(&self) -> bool {
        self.alsa_card.is_device_in_use()
    }
    pub fn wait_unlock_audio_dev(&self) -> Result<()> {
        self.alsa_card.wait_unlock_audio_dev()
    }

    pub fn set_volume(&self, value: i64) -> Result<Volume> {
        Ok(self.volume_ctrl_device.set_vol(value))
    }
    pub fn volume_up(&self) -> Result<Volume> {
        Ok(self.volume_ctrl_device.vol_up())
    }
    pub fn volume_down(&self) -> Result<Volume> {
        Ok(self.volume_ctrl_device.vol_down())
    }
}
