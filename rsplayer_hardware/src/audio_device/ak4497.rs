use anyhow::Result;
use api_models::common::GainLevel;

use api_models::common::{FilterType, Volume};
use api_models::settings::DacSettings;
use std::thread;
use std::time::Duration;

use crate::mcu::gpio;
use crate::mcu::gpio::GPIO_PIN_OUTPUT_DAC_PDN_RST;
use crate::mcu::i2c::I2CHelper;
use log::{debug, error, info};

use super::VolumeControlDevice;

pub struct DacAk4497 {
    i2c_helper: I2CHelper,
    volume_step: u8,
}
unsafe impl Sync for DacAk4497 {}

impl VolumeControlDevice for DacAk4497 {
    fn vol_up(&self) -> Volume {
        let curr_val = self.i2c_helper.read_register(3).expect("Register read failed");
        curr_val
            .checked_add(self.volume_step)
            .map_or_else(|| self.set_vol(255), |new_val| self.set_vol(i64::from(new_val)))
    }

    fn vol_down(&self) -> Volume {
        let curr_val = self.i2c_helper.read_register(3).expect("Register read failed");
        curr_val
            .checked_sub(self.volume_step)
            .map_or_else(|| self.set_vol(0), |new_val| self.set_vol(i64::from(new_val)))
    }

    fn get_vol(&self) -> Volume {
        let curr_val = self.i2c_helper.read_register(3).expect("Register read failed");
        Volume {
            step: i64::from(self.volume_step),
            min: 0,
            max: 255,
            current: i64::from(curr_val),
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn set_vol(&self, value: i64) -> Volume {
        self.i2c_helper.write_register(3, value as u8);
        self.i2c_helper.write_register(4, value as u8);
        Volume {
            current: value,
            max: 255,
            min: 0,
            step: i64::from(self.volume_step),
        }
    }
}

#[allow(dead_code)]
const fn is_bit_set(input: u8, n: u8) -> bool {
    if n < 8 {
        input & (1 << n) != 0
    } else {
        false
    }
}

impl DacAk4497 {
    pub fn new(volume_state: &Volume, settings: &DacSettings) -> Result<Box<Self>> {
        let dac = Self {
            i2c_helper: I2CHelper::new(settings.i2c_address)?,
            volume_step: settings.volume_step,
        };
        dac.initialize(volume_state, settings)?;
        Ok(Box::new(dac))
    }

    fn initialize(&self, volume: &Volume, dac_settings: &DacSettings) -> Result<()> {
        self.debug_registers("before pdn");
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
                self.i2c_helper.read_register(0)?;
            }
        }

        self.debug_registers("before init");

        // control 1
        // ACKS = 1 (ignored when AFSD = 1)
        // AFSD = 1
        // PCM mode normal, mode 3, 16-bit I2S Compatible (this mode is required for FifoPiMa output, otherwise it works in default mode which is tested with WaveIO)
        self.i2c_helper.write_register(0, 0b1000_0111);

        // control 2
        // DEM[1:0] = 01 - De-emphasis Filter Control on 44.1 kHz
        self.i2c_helper.write_register(1, 0b0000_0010);
        // invert signal left and right channel
        self.i2c_helper.change_bit(5, 7, true);
        self.i2c_helper.change_bit(5, 6, true);
        self.set_vol(volume.current);
        self.filter(dac_settings.filter);
        self.set_gain(dac_settings.gain);
        self.hi_load(dac_settings.heavy_load);
        self.change_sound_setting(dac_settings.sound_sett)?;
        self.debug_registers("after init");
        Ok(())
    }

    fn debug_registers(&self, msg: &str) {
        debug!("Dac registry {}", msg);
        if let Ok(f) = self.get_reg_values() {
            for s in &f {
                debug!("{}", s);
            }
        }
    }
    fn get_reg_values(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        for rg in 0..15 {
            let val = self.i2c_helper.read_register(rg)?;
            result.push(format!("Register {rg} has value {val:#010b} ({val})"));
        }
        Ok(result)
    }

    pub fn change_sound_setting(&self, setting_no: u8) -> Result<u8> {
        match setting_no {
            1 => {
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 1, false);
                self.i2c_helper.change_bit(8, 2, false);
            }
            2 => {
                self.i2c_helper.change_bit(8, 0, true);
                self.i2c_helper.change_bit(8, 1, false);
                self.i2c_helper.change_bit(8, 2, false);
            }
            3 => {
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 1, true);
                self.i2c_helper.change_bit(8, 2, false);
            }
            4 => {
                self.i2c_helper.change_bit(8, 0, true);
                self.i2c_helper.change_bit(8, 1, true);
                self.i2c_helper.change_bit(8, 2, false);
            }
            5 => {
                self.i2c_helper.change_bit(8, 0, false);
                self.i2c_helper.change_bit(8, 1, false);
                self.i2c_helper.change_bit(8, 2, true);
            }
            _ => return Err(anyhow::format_err!("Unknown setting no {}", setting_no)),
        }
        Ok(setting_no)
    }

    pub fn filter(&self, typ: FilterType) -> FilterType {
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
        typ
    }

    pub fn hi_load(&self, flag: bool) {
        self.i2c_helper.change_bit(8, 3, flag);
    }

    pub fn set_gain(&self, level: GainLevel) {
        match level {
            GainLevel::V25 => self.i2c_helper.write_register(7, 0b0000_0101),
            GainLevel::V28 => self.i2c_helper.write_register(7, 0b0000_0001),
            GainLevel::V375 => self.i2c_helper.write_register(7, 0b0000_1001),
        }
    }

    #[allow(dead_code)]
    fn reset(&self) {
        self.i2c_helper.change_bit(0, 0, false);
        thread::sleep(Duration::from_millis(50));
        self.i2c_helper.change_bit(0, 0, true);
    }

    #[allow(dead_code)]
    pub fn dsd_pcm(&self, dsd: bool) {
        if dsd {
            // switch to DSD mode
            self.i2c_helper.change_bit(0, 0, false);
            thread::sleep(Duration::from_millis(50));
            self.i2c_helper.change_bit(2, 7, true);
            thread::sleep(Duration::from_millis(50));
            self.i2c_helper.change_bit(0, 0, true);
            self.i2c_helper.write_register(6, 0b1001_1001);
            self.i2c_helper.write_register(9, 0b0000_0001);
        } else {
            self.i2c_helper.change_bit(2, 7, false);
            self.reset();
            self.i2c_helper.write_register(0, 0b1001_0111);
        }
    }

    #[allow(dead_code)]
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
