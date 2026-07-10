//! [`AudioInterfaceService`] — owns the active [`VolumeControlDevice`]
//! chosen from settings and serializes access to it; the target of all
//! `SystemCommand` volume operations.

use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use anyhow::Result;
use api_models::common::Volume;

use config::ArcConfiguration;

use super::VolumeControlDevice;
#[cfg(feature = "alsa")]
use super::alsa::AlsaMixer;
use super::software_gain::SoftwareGainVolumeControlDevice;

pub type ArcAudioInterfaceSvc = Arc<AudioInterfaceService>;

use std::sync::Mutex;

pub struct AudioInterfaceService {
    volume_ctrl_device: Mutex<Box<dyn VolumeControlDevice + Sync + Send>>,
}

use super::NoOpVolumeControlDevice;
use super::pipewire::PipewireVolumeControlDevice;
use super::rsp_firmware::RSPlayerFirmwareVolumeControlDevice;
use api_models::common::VolumeCrtlType;

use crate::usb::ArcUsbService;

impl AudioInterfaceService {
    pub fn new(config: &ArcConfiguration, usb_service: Option<ArcUsbService>, software_gain_level: Arc<AtomicU8>) -> Result<Arc<Self>> {
        let settings = config.get_settings();

        #[cfg(feature = "alsa")]
        let (settings, card_index) = {
            let mut settings = settings;
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
            (settings, card_index)
        };

        let mut volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> = if settings.usb_settings.enabled {
            if usb_service.is_none() {
                return Err(anyhow::anyhow!("USB service is required for RSPlayerFirmware volume control."));
            }
            Box::new(RSPlayerFirmwareVolumeControlDevice::new(
                usb_service.expect("usb_service checked above"),
            ))
        } else {
            match settings.volume_ctrl_settings.ctrl_device {
                #[cfg(feature = "alsa")]
                VolumeCrtlType::Alsa => AlsaMixer::new(card_index, settings.volume_ctrl_settings.alsa_mixer),
                #[cfg(not(feature = "alsa"))]
                VolumeCrtlType::Alsa => {
                    log::warn!("ALSA volume control requested but ALSA support is not compiled in, using NoOp");
                    Box::new(NoOpVolumeControlDevice)
                }
                VolumeCrtlType::Pipewire => Box::new(PipewireVolumeControlDevice::new()),
                VolumeCrtlType::Software => SoftwareGainVolumeControlDevice::new(software_gain_level),
                VolumeCrtlType::Off => Box::new(NoOpVolumeControlDevice),
            }
        };

        // Restore the saved volume. When none was saved, leave the device at
        // its current level — forcing 0 here would e.g. mute the user's whole
        // desktop when the PipeWire default sink is the control device.
        if let Some(saved) = settings.volume_ctrl_settings.saved_volume {
            volume_ctrl_device.set_vol(saved);
        }

        Ok(Arc::new(Self {
            volume_ctrl_device: Mutex::new(volume_ctrl_device),
        }))
    }

    pub fn get_volume(&self) -> Volume {
        self.volume_ctrl_device.lock().expect("lock poisoned").get_vol()
    }

    pub fn set_volume(&self, value: u8) -> Volume {
        self.volume_ctrl_device.lock().expect("lock poisoned").set_vol(value)
    }
    pub fn volume_up(&self) -> Volume {
        self.volume_ctrl_device.lock().expect("lock poisoned").vol_up()
    }
    pub fn volume_down(&self) -> Volume {
        self.volume_ctrl_device.lock().expect("lock poisoned").vol_down()
    }
}
