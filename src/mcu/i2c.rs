use rpi_embedded::i2c::I2c;
use std::thread;
use std::time::Duration;
pub struct I2CHelper {
    i2c: I2c,
}

impl I2CHelper {
    pub fn new(address: u16) -> Self {
        let mut i2c = I2c::new().expect("i2c failed in initialization");
        i2c.set_slave_address(address)
            .expect("slave address failed");
        Self { i2c }
    }

    pub(crate) fn read_register(self: &Self, reg_addr: u8) -> Result<u8, failure::Error> {
        let mut out = [0u8];
        match self.i2c.cmd_read(reg_addr, &mut out) {
            Ok(_) => Ok(out[0]),
            Err(_err) => Err(failure::err_msg("error")),
        }
    }

    pub(crate) fn write_register(self: &Self, reg_addr: u8, value: u8) {
        thread::sleep(Duration::from_millis(20));
        self.i2c
            .cmd_write(reg_addr, value)
            .expect("Can not write to register");
    }

    pub(crate) fn change_bit(self: &Self, reg_addr: u8, bit_position: u8, bit_value: bool) {
        let reg_val = self
            .read_register(reg_addr)
            .expect("Failed to read register");
        let mask = 1 << bit_position;
        let new_val;
        if bit_value {
            new_val = reg_val | mask;
        } else {
            new_val = reg_val & !mask;
        }
        thread::sleep(Duration::from_millis(20));
        trace!(
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
