use std::thread;
use std::time::Duration;

use crate::common::{DacStatus, FilterType, GainLevel, Result};
use crate::config::DacSettings;
use crate::mcu::gpio;
use crate::mcu::gpio::GPIO_PIN_OUTPUT_DAC_PDN_RST;
use crate::mcu::i2c::I2CHelper;
use mockall::automock;

pub struct Dac {
    i2c_helper: I2CHelper,
    volume_step: u8,
}

unsafe impl Send for Dac {}

unsafe impl Sync for Dac {}

#[automock]
impl Dac {
    pub fn new(dac_state: DacStatus, settings: &DacSettings) -> Result<Self> {
        let dac = Self {
            i2c_helper: I2CHelper::new(settings.i2c_address),
            volume_step: settings.volume_step,
        };
        dac.initialize(dac_state)?;
        return Ok(dac);
    }

    fn initialize(self: &Self, dac_state: DacStatus) -> Result<()> {
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
        self.set_vol(dac_state.volume).expect("error");
        self.filter(dac_state.filter).expect("error");
        self.set_gain(dac_state.gain).expect("error");
        self.hi_load(dac_state.heavy_load).expect("error");
        self.change_sound_setting(dac_state.sound_sett)
            .expect("error");
        //self.soft_mute(dac_state.muted);
        trace!("Dac registry After init");
        self.get_reg_values()
            .expect("Can not read dac registry")
            .into_iter()
            .for_each(|r| trace!("{}", r));
        Ok(())
    }

    pub fn change_sound_setting(self: &Self, setting_no: u8) -> Result<u8> {
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
            _ => return Err(failure::format_err!("Unknown setting no")),
        }
        Ok(setting_no)
    }

    fn get_reg_values(self: &Self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        for rg in 0..15 {
            let val = self.i2c_helper.read_register(rg)?;
            result.push(format!("Register {} has value {:#010b} ({})", rg, val, val));
        }
        Ok(result)
    }

    pub fn set_vol(self: &Self, value: u8) -> Result<u8> {
        self.i2c_helper.write_register(3, value);
        self.i2c_helper.write_register(4, value);
        Ok(value)
    }

    pub fn vol_down(self: &Self) -> Result<u8> {
        let curr_val = self.i2c_helper.read_register(3)?;
        if let Some(new_val) = curr_val.checked_sub(self.volume_step) {
            self.set_vol(new_val)
        } else {
            self.set_vol(0)
        }
    }

    pub fn vol_up(self: &Self) -> Result<u8> {
        let curr_val = self.i2c_helper.read_register(3)?;
        if let Some(new_val) = curr_val.checked_add(self.volume_step) {
            self.set_vol(new_val)
        } else {
            self.set_vol(255)
        }
    }

    pub fn filter(self: &Self, typ: FilterType) -> Result<FilterType> {
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

    pub fn hi_load(self: &Self, flag: bool) -> Result<bool> {
        self.i2c_helper.change_bit(8, 3, flag);
        Ok(flag)
    }

    pub fn set_gain(self: &Self, level: GainLevel) -> Result<GainLevel> {
        match level {
            GainLevel::V25 => self.i2c_helper.write_register(7, 0b0000_0101),
            GainLevel::V28 => self.i2c_helper.write_register(7, 0b0000_0001),
            GainLevel::V375 => self.i2c_helper.write_register(7, 0b0000_1001),
        }
        Ok(level)
    }

    fn reset(self: &Self) {
        self.i2c_helper.change_bit(0, 0, false);
        thread::sleep(Duration::from_millis(20));
        self.i2c_helper.change_bit(0, 0, true);
    }

    pub fn dsd_pcm(self: &Self, dsd: bool) {
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

    pub fn soft_mute(self: &Self, flag: bool) {
        self.i2c_helper.change_bit(1, 0, flag);
    }
}

fn press_pdn_button() {
    gpio::set_output_pin_value(GPIO_PIN_OUTPUT_DAC_PDN_RST, false);
    thread::sleep(Duration::from_millis(50));
    gpio::set_output_pin_value(GPIO_PIN_OUTPUT_DAC_PDN_RST, true);
    thread::sleep(Duration::from_millis(50));
}
