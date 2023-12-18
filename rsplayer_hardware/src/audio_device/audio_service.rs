use std::sync::Arc;

use crate::mcu::gpio::{self, GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY};
use anyhow::Result;
use api_models::common::{Volume, VolumeCrtlType};
use api_models::state::AudioOut;
use gpio_cdev::LineHandle;
use rsplayer_config::ArcConfiguration;

use super::ak4497::DacAk4497;
use super::alsa::AlsaMixer;
use super::VolumeControlDevice;

pub type ArcAudioInterfaceSvc = Arc<AudioInterfaceService>;

pub struct AudioInterfaceService {
    volume_ctrl_device: Box<dyn VolumeControlDevice + Sync + Send>,
    output_selector_pin: Option<LineHandle>,
}

impl AudioInterfaceService {
    pub fn new(config: &ArcConfiguration) -> Result<Self> {
        let settings = config.get_settings();
        let volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> =
            if settings.volume_ctrl_settings.ctrl_device == VolumeCrtlType::Dac && settings.dac_settings.enabled {
                DacAk4497::new(&config.get_streamer_state().volume_state, &settings.dac_settings)?
            } else {
                AlsaMixer::new(
                    settings.alsa_settings.output_device.card_index,
                    settings.volume_ctrl_settings.alsa_mixer,
                )
            };
        let line_handle = if settings.output_selector_settings.enabled {
            // restore last output state
            let out_sel_pin = gpio::get_output_pin_handle(GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY)?;
            if config.get_streamer_state().selected_audio_output == AudioOut::SPKR {
                out_sel_pin.set_value(0)?;
            } else {
                out_sel_pin.set_value(1)?;
            };
            Some(out_sel_pin)
        } else {
            None
        };
        Ok(Self {
            volume_ctrl_device,
            output_selector_pin: line_handle,
        })
    }

    pub fn set_volume(&self, value: i64) -> Volume {
        self.volume_ctrl_device.set_vol(value)
    }
    pub fn volume_up(&self) -> Volume {
        self.volume_ctrl_device.vol_up()
    }
    pub fn volume_down(&self) -> Volume {
        self.volume_ctrl_device.vol_down()
    }
    pub fn toggle_output(&self) -> Option<AudioOut> {
        self.output_selector_pin.as_ref().map(|out_sel_pin| {
            if out_sel_pin.get_value().unwrap() == 0 {
                _ = out_sel_pin.set_value(1);
                AudioOut::HEAD
            } else {
                _ = out_sel_pin.set_value(0);
                AudioOut::SPKR
            }
        })
    }
}
