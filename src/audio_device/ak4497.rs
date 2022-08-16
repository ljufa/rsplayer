use crate::common::Result;
use api_models::common::GainLevel;

use api_models::common::{FilterType, Volume};
use api_models::settings::*;
use api_models::state::VolumeState;
use std::time::Duration;

use crate::mcu::gpio;
use crate::mcu::gpio::GPIO_PIN_OUTPUT_DAC_PDN_RST;
use crate::mcu::i2c::I2CHelper;

use super::VolumeControlDevice;

pub struct DacAk4497 {
    i2c_helper: I2CHelper,
    volume_step: u8,
}
unsafe impl Sync for DacAk4497 {}

impl VolumeControlDevice for DacAk4497 {
    fn vol_up(&self) -> Volume {
        let curr_val = self
            .i2c_helper
            .read_register(3)
            .expect("Register read failed");
        if let Some(new_val) = curr_val.checked_add(self.volume_step) {
            self.set_vol(new_val as i64)
        } else {
            self.set_vol(255)
        }
    }

    fn vol_down(&self) -> Volume {
        let curr_val = self
            .i2c_helper
            .read_register(3)
            .expect("Register read failed");
        if let Some(new_val) = curr_val.checked_sub(self.volume_step) {
            self.set_vol(new_val as i64)
        } else {
            self.set_vol(0)
        }
    }

    fn get_vol(&self) -> Volume {
        let curr_val = self
            .i2c_helper
            .read_register(3)
            .expect("Register read failed");
        Volume {
            step: self.volume_step as i64,
            min: 0,
            max: 255,
            current: curr_val as i64,
        }
    }

    fn set_vol(&self, value: i64) -> Volume {
        self.i2c_helper.write_register(3, value as u8);
        self.i2c_helper.write_register(4, value as u8);
        Volume {
            current: value,
            max: 255,
            min: 0,
            step: self.volume_step as i64,
        }
    }
}

impl DacAk4497 {
    pub fn new(dac_state: VolumeState, settings: &DacSettings) -> Result<Box<Self>> {
        let dac = Self {
            i2c_helper: I2CHelper::new(settings.i2c_address),
            volume_step: settings.volume_step,
        };
        dac.initialize(dac_state, settings)?;
        Ok(Box::new(dac))
    }

    fn initialize(&self, dac_state: VolumeState, dac_settings: &DacSettings) -> Result<()> {
        // reset dac
        press_pdn_button();
        // try talking to dac,
        match self.i2c_helper.read_register(0) {
            Ok(_) => {
                info!("Dac available on i2c bus");
            }
            Err(_e) => {
                error!("Dac not available on i2c bus, sending power down command.");
                // if not available powerdown dac pin
                press_pdn_button();
                self.i2c_helper
                    .read_register(0)
                    .expect("Dac not available after restart");
            }
        }
        trace!("Dac registry before init");
        self.get_reg_values()
            .expect("Can not read dac registry")
            .into_iter()
            .for_each(|r| trace!("{}", r));

        self.i2c_helper.write_register(0, 0b1000_1111);
        self.i2c_helper.write_register(1, 0b1010_0010);
        self.set_vol(dac_state.volume);
        self.filter(dac_settings.filter)?;
        self.set_gain(dac_settings.gain)?;
        self.hi_load(dac_settings.heavy_load)?;
        self.change_sound_setting(dac_settings.sound_sett)?;
        trace!("Dac registry After init");
        self.get_reg_values()
            .expect("Can not read dac registry")
            .into_iter()
            .for_each(|r| trace!("{}", r));
        Ok(())
    }

    pub fn change_sound_setting(&self, setting_no: u8) -> Result<u8> {
        match setting_no {
            1 => {
                self.i2c_helper.change_bit(8, 1, false);
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 2, false);
            }
            2 => {
                self.i2c_helper.change_bit(8, 1, false);
                self.i2c_helper.change_bit(8, 0, true);
                self.i2c_helper.change_bit(8, 2, false);
            }
            3 => {
                self.i2c_helper.change_bit(8, 1, true);
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 2, false);
            }
            4 => {
                self.i2c_helper.change_bit(8, 1, true);
                self.i2c_helper.change_bit(8, 0, true);
                self.i2c_helper.change_bit(8, 2, false);
            }
            5 => {
                self.i2c_helper.change_bit(8, 2, true);
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 1, false);
            }
            _ => return Err(failure::format_err!("Unknown setting no {}", setting_no)),
        }
        Ok(setting_no)
    }

    pub fn get_reg_values(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        for rg in 0..15 {
            let val = self.i2c_helper.read_register(rg)?;
            result.push(format!("Register {} has value {:#010b} ({})", rg, val, val));
        }
        Ok(result)
    }

    pub fn filter(&self, typ: FilterType) -> Result<FilterType> {
        match typ {
            FilterType::SharpRollOff => {
                self.i2c_helper.change_bit(5, 0, false);
                self.i2c_helper.change_bit(1, 5, false);
                self.i2c_helper.change_bit(2, 0, false);
            }
            FilterType::SlowRollOff => {
                self.i2c_helper.change_bit(5, 0, false);
                self.i2c_helper.change_bit(1, 5, false);
                self.i2c_helper.change_bit(2, 0, true);
            }
            FilterType::ShortDelaySharpRollOff => {
                self.i2c_helper.change_bit(5, 0, false);
                self.i2c_helper.change_bit(1, 5, true);
                self.i2c_helper.change_bit(2, 0, false);
            }
            FilterType::ShortDelaySlowRollOff => {
                self.i2c_helper.change_bit(5, 0, false);
                self.i2c_helper.change_bit(1, 5, true);
                self.i2c_helper.change_bit(2, 0, true);
            }
            FilterType::SuperSlow => {
                self.i2c_helper.change_bit(5, 0, true);
                self.i2c_helper.change_bit(1, 5, false);
                self.i2c_helper.change_bit(2, 0, false);
            }
        }
        Ok(typ)
    }

    pub fn hi_load(&self, flag: bool) -> Result<bool> {
        self.i2c_helper.change_bit(8, 3, flag);
        Ok(flag)
    }

    pub fn set_gain(&self, level: GainLevel) -> Result<GainLevel> {
        match level {
            GainLevel::V25 => self.i2c_helper.write_register(7, 0b0000_0101),
            GainLevel::V28 => self.i2c_helper.write_register(7, 0b0000_0001),
            GainLevel::V375 => self.i2c_helper.write_register(7, 0b0000_1001),
        }
        Ok(level)
    }

    fn reset(&self) {
        self.i2c_helper.change_bit(0, 0, false);
        self.i2c_helper.change_bit(0, 0, true);
    }

    pub fn dsd_pcm(&self, dsd: bool) {
        // ChangeBit(ak4490, 0x01, 0, true);         // Enable soft mute
        // ChangeBit(ak4490, 0x02, 7, true);         // Set To DSD Mode
        // WriteRegister(ak4490,0x00,B00000000);     // Reset
        // WriteRegister(ak4490,0x00,B00000001);     // Normal operation
        // WriteRegister(ak4490,0x00,B10001111);     // Set To Master Clock Frequency Auto / 32Bit I2S Mode
        // WriteRegister(ak4490,0x06,B10001001);     // Set To DSD Data Mute / DSD Mute Control / DSD Mute Release
        // WriteRegister(ak4490,0x09,B00000001);     // Set To DSD Sampling Speed Control
        // ChangeBit(ak4490, 0x01, 0, false);        // Disable soft mute
        let reg_val = self
            .i2c_helper
            .read_register(2)
            .expect("Can not read register 2");
        if dsd {
            self.soft_mute(true);
            self.i2c_helper.write_register(2, reg_val | 0b1000_0000);
            self.i2c_helper.write_register(0, 0b0000_0000);
            self.i2c_helper.write_register(0, 0b0000_0001);
            self.i2c_helper.write_register(0, 0b1000_1111);
            self.i2c_helper.write_register(6, 0b1001_1001);
            self.i2c_helper.write_register(9, 0b0000_0001);
            self.soft_mute(false);
        } else {
            self.i2c_helper.write_register(2, reg_val & 0b0111_1111);
        }
        self.reset();
    }

    pub fn soft_mute(&self, flag: bool) {
        self.i2c_helper.change_bit(1, 0, flag);
    }
}

fn press_pdn_button() {
    gpio::set_output_pin_value(GPIO_PIN_OUTPUT_DAC_PDN_RST, false);
    std::thread::sleep(Duration::from_millis(30));
    gpio::set_output_pin_value(GPIO_PIN_OUTPUT_DAC_PDN_RST, true);
    std::thread::sleep(Duration::from_millis(30));
}
