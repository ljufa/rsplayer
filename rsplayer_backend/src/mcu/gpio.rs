// Gpio uses BCM pin numbering. BCM GPIO 23 is tied to physical pin 16.
use gpio_cdev::{chips, Chip, LineDirection, LineHandle, LineRequestFlags};

use crate::common::Result;

pub const GPIO_PIN_OUT_AUDIO_OUT_SELECTOR_RELAY: u32 = 9;

pub const GPIO_PIN_OUTPUT_LCD_RST: u64 = 25;
pub const GPIO_PIN_OUTPUT_DAC_PDN_RST: u32 = 14;

pub fn set_output_pin_value(pin_no: u32, value: bool) {
    if let Ok(handle) = get_output_pin_handle(pin_no) {
        if value {
            _ = handle.set_value(1);
        } else {
            _ = handle.set_value(0);
        }
    }
}

pub fn get_output_pin_handle(pin_no: u32) -> Result<LineHandle> {
    let mut chip = Chip::new("/dev/gpiochip0")?;
    Ok(chip
        .get_line(pin_no)?
        .request(LineRequestFlags::OUTPUT, 0, "rsplayer")?)
}
#[allow(dead_code)]
pub fn get_lines(pin_no: &[u32]) -> gpio_cdev::Lines {
    let mut chip = Chip::new("/dev/gpiochip0").expect("Gpio chip not present");
    chip.get_lines(pin_no).unwrap()
}

#[allow(dead_code)]
pub fn lsgpio() {
    let chip_iterator = match chips() {
        Ok(chips) => chips,
        Err(e) => {
            println!("Failed to get chip iterator: {:?}", e);
            return;
        }
    };

    for chip in chip_iterator.flatten() {
        println!(
            "GPIO chip: {}, \"{}\", \"{}\", {} GPIO Lines",
            chip.path().to_string_lossy(),
            chip.name(),
            chip.label(),
            chip.num_lines()
        );
        for line in chip.lines() {
            match line.info() {
                Ok(info) => {
                    let mut flags = vec![];

                    if info.is_kernel() {
                        flags.push("kernel");
                    }

                    if info.direction() == LineDirection::Out {
                        flags.push("output");
                    }

                    if info.is_active_low() {
                        flags.push("active-low");
                    }
                    if info.is_open_drain() {
                        flags.push("open-drain");
                    }
                    if info.is_open_source() {
                        flags.push("open-source");
                    }

                    let usage = if !flags.is_empty() {
                        format!("[{}]", flags.join(" "))
                    } else {
                        "".to_owned()
                    };

                    println!(
                        "\tline {lineno:>3}: {name} {consumer} {usage}",
                        lineno = info.line().offset(),
                        name = info.name().unwrap_or("unused"),
                        consumer = info.consumer().unwrap_or("unused"),
                        usage = usage,
                    );
                }
                Err(e) => println!("\tError getting line info: {:?}", e),
            }
        }
        println!();
    }
}
