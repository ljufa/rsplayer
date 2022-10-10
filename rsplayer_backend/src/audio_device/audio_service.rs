use crate::common::{MutArcConfiguration, Result};
use crate::mcu::gpio::{self, GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY};
use api_models::common::{Volume, VolumeCrtlType};
use api_models::state::AudioOut;
use gpio_cdev::LineHandle;

use super::ak4497::DacAk4497;
use super::alsa::AlsaMixer;
use super::VolumeControlDevice;

pub struct AudioInterfaceService {
    volume_ctrl_device: Box<dyn VolumeControlDevice + Sync + Send>,
    output_selector_pin: Option<LineHandle>,
}

impl AudioInterfaceService {
    pub fn new(config: &MutArcConfiguration) -> Result<Self> {
        let mut config = config.lock().expect("Unable to lock config");
        let settings = config.get_settings();
        let volume_ctrl_device: Box<dyn VolumeControlDevice + Send + Sync> =
            if settings.volume_ctrl_settings.ctrl_device == VolumeCrtlType::Dac
                && settings.dac_settings.enabled
            {
                DacAk4497::new(
                    config.get_streamer_status().volume_state,
                    &settings.dac_settings,
                )?
            } else {
                AlsaMixer::new(settings.alsa_settings.device_name)
            };
        let line_handle = if settings.output_selector_settings.enabled {
            // restore last output state
            let out_sel_pin = gpio::get_output_pin_handle(GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY)?;
            match config.get_streamer_status().selected_audio_output {
                AudioOut::SPKR => out_sel_pin.set_value(0)?,
                AudioOut::HEAD => out_sel_pin.set_value(1)?,
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
        if let Some(out_sel_pin) = self.output_selector_pin.as_ref() {
            let out = if out_sel_pin.get_value().unwrap() == 0 {
                let _ = out_sel_pin.set_value(1);
                AudioOut::HEAD
            } else {
                let _ = out_sel_pin.set_value(0);
                AudioOut::SPKR
            };
            Some(out)
        } else {
            None
        }
    }
}
