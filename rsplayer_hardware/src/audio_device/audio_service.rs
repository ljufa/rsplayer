use std::sync::Arc;

use anyhow::Result;
use api_models::common::Volume;

use rsplayer_config::ArcConfiguration;

use super::alsa::AlsaMixer;
use super::VolumeControlDevice;

pub type ArcAudioInterfaceSvc = Arc<AudioInterfaceService>;

use std::sync::Mutex;

pub struct AudioInterfaceService {
    volume_ctrl_device: Mutex<Box<dyn VolumeControlDevice + Sync + Send>>,
}

use super::rsp_firmware::RSPlayerFirmwareVolumeControlDevice;
use super::NoOpVolumeControlDevice;
use api_models::common::VolumeCrtlType;

use crate::usb::ArcUsbService;

impl AudioInterfaceService {
    pub fn new(config: &ArcConfiguration, usb_service: Option<ArcUsbService>) -> Result<Self> {
        let mut settings = config.get_settings();
        let cards = crate::audio_device::alsa::get_all_cards();
        let card_index = cards
            .iter()
            .find(|c| c.id == settings.alsa_settings.output_device.card_id)
            .map_or(0, |card| card.index);

        if let Some(mixer_name) = &settings.volume_ctrl_settings.alsa_mixer_name {
            for card in &cards {
                if let Some(mixer) = card.mixers.iter().find(|m| &m.name == mixer_name) {
                    settings.volume_ctrl_settings.alsa_mixer = Some(mixer.clone());
                    break;
                }
            }
        }
        let volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> = if settings.usb_settings.enabled {
            if usb_service.is_none() {
                return Err(anyhow::anyhow!(
                    "USB service is required for RSPlayerFirmware volume control."
                ));
            }
            Box::new(RSPlayerFirmwareVolumeControlDevice::new(usb_service.unwrap()))
        } else {
            match settings.volume_ctrl_settings.ctrl_device {
                VolumeCrtlType::Alsa => AlsaMixer::new(card_index, settings.volume_ctrl_settings.alsa_mixer),
                VolumeCrtlType::Off => Box::new(NoOpVolumeControlDevice),
            }
        };

        Ok(Self {
            volume_ctrl_device: Mutex::new(volume_ctrl_device),
        })
    }

    pub fn get_volume(&self) -> Volume {
        self.volume_ctrl_device.lock().unwrap().get_vol()
    }

    pub fn set_volume(&self, value: u8) -> Volume {
        self.volume_ctrl_device.lock().unwrap().set_vol(value)
    }
    pub fn volume_up(&self) -> Volume {
        self.volume_ctrl_device.lock().unwrap().vol_up()
    }
    pub fn volume_down(&self) -> Volume {
        self.volume_ctrl_device.lock().unwrap().vol_down()
    }
}
