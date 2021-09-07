// Gpio uses BCM pin numbering. BCM GPIO 23 is tied to physical pin 16.
use gpio_cdev::{Chip, LineHandle, LineRequestFlags};

pub const GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY: u32 = 9;

pub const GPIO_PIN_OUTPUT_LCD_RST: u64 = 25;
pub const GPIO_PIN_OUTPUT_DAC_PDN_RST: u32 = 14;

pub fn set_output_pin_value(pin_no: u32, value: bool) {
    let handle = get_output_pin_handle(pin_no);
    if value {
        handle.set_value(1).expect("Error");
    } else {
        handle.set_value(0).expect("Error");
    }
}

pub fn get_output_pin_handle(pin_no: u32) -> LineHandle {
    let mut chip = Chip::new("/dev/gpiochip0").expect("Gpio chip not present");
    let handle = chip
        .get_line(pin_no)
        .expect(format!("Pin {} can not be opened", pin_no).as_str())
        .request(LineRequestFlags::OUTPUT, 0, "dplay")
        .expect("Request to pin failed");
    handle
}
