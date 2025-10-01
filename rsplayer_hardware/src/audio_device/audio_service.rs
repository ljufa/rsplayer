use std::sync::Arc;

use anyhow::Result;
use api_models::common::Volume;

use rsplayer_config::ArcConfiguration;

use super::alsa::AlsaMixer;
use super::VolumeControlDevice;

pub type ArcAudioInterfaceSvc = Arc<AudioInterfaceService>;

pub struct AudioInterfaceService {
    volume_ctrl_device: Box<dyn VolumeControlDevice + Sync + Send>,
}

impl AudioInterfaceService {
    pub fn new(config: &ArcConfiguration) -> Result<Self> {
        let settings = config.get_settings();
        let volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> = AlsaMixer::new(
            settings.alsa_settings.output_device.card_index,
            settings.volume_ctrl_settings.alsa_mixer,
            &config.get_streamer_state().volume_state,
        );

        Ok(Self { volume_ctrl_device })
    }

    pub fn set_volume(&self, value: u8) -> Volume {
        self.volume_ctrl_device.set_vol(value)
    }
    pub fn volume_up(&self) -> Volume {
        self.volume_ctrl_device.vol_up()
    }
    pub fn volume_down(&self) -> Volume {
        self.volume_ctrl_device.vol_down()
    }
}
