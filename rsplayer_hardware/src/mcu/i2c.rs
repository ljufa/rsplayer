use rpi_embedded::i2c::I2c;

use anyhow::Result;
use log::debug;

pub struct I2CHelper {
    i2c: I2c,
}

impl I2CHelper {
    pub fn new(address: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
        i2c.set_slave_address(address)?;
        Ok(Self { i2c })
    }

    pub(crate) fn read_register(&self, reg_addr: u8) -> Result<u8> {
        let mut out = [0u8];
        match self.i2c.cmd_read(reg_addr, &mut out) {
            Ok(_) => Ok(out[0]),
            Err(_err) => Err(anyhow::format_err!("error")),
        }
    }

    pub(crate) fn write_register(&self, reg_addr: u8, value: u8) {
        debug!("I2C write reg_addr:{}, value: {}", reg_addr, value);
        self.i2c
            .cmd_write(reg_addr, value)
            .expect("Can not write to register");
    }

    pub(crate) fn change_bit(&self, reg_addr: u8, bit_position: u8, bit_value: bool) {
        let reg_val = self
            .read_register(reg_addr)
            .expect("Failed to read register");
        let mask = 1 << bit_position;
        let new_val = if bit_value {
            reg_val | mask
        } else {
            reg_val & !mask
        };
        debug!(
            "Change bit {}={} in registry {}. From {:#010b} to {:#010b}",
            bit_position,
            bit_value,
            reg_addr,
            reg_val,
            new_val
        );
        self.write_register(reg_addr, new_val);
    }
}
