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

use crate::uart::service::ArcUartService;

impl AudioInterfaceService {
    pub fn new(config: &ArcConfiguration, uart_service: Option<ArcUartService>) -> Result<Self> {
        let settings = config.get_settings();
        let volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> = match settings.volume_ctrl_settings.ctrl_device {
            VolumeCrtlType::Alsa => AlsaMixer::new(
                &settings.alsa_settings.output_device.card_id,
                settings.volume_ctrl_settings.alsa_mixer,
            ),
            VolumeCrtlType::RSPlayerFirmware => {
                if uart_service.is_none() {
                    return Err(anyhow::anyhow!("UART service is required for RSPlayerFirmware volume control."));
                }
                Box::new(RSPlayerFirmwareVolumeControlDevice::new(uart_service.unwrap()))
            },
            _ => Box::new(NoOpVolumeControlDevice),
        };

        Ok(Self { volume_ctrl_device: Mutex::new(volume_ctrl_device) })
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
