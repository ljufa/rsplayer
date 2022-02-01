use std::time::Duration;

use embedded_graphics::{
    mono_font::{ascii::FONT_5X8, ascii::FONT_6X12, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use embedded_hal::blocking::delay::DelayUs;

use linux_embedded_hal::spidev::SpiModeFlags;
use linux_embedded_hal::spidev::SpidevOptions;
use linux_embedded_hal::sysfs_gpio::Direction;
use linux_embedded_hal::Spidev;
use linux_embedded_hal::{Delay, Pin};

use tokio::sync::broadcast::Receiver;
use unidecode::unidecode;

use crate::common::{CommandEvent, PlayerStatus};
use crate::config::StreamerStatus;
use crate::mcu::gpio::GPIO_PIN_OUTPUT_LCD_RST;
use crate::monitor::myst7920::ST7920;

pub fn start(mut state_changes_receiver: Receiver<CommandEvent>) {
    tokio::task::spawn(async move {
        let mut delay = Delay;
        let mut spi = Spidev::open("/dev/spidev0.0").expect("error initializing SPI");
        let options = SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(800000)
            .mode(SpiModeFlags::SPI_CS_HIGH)
            .build();
        spi.configure(&options).expect("error configuring SPI");
        let rst_pin = Pin::new(GPIO_PIN_OUTPUT_LCD_RST);
        rst_pin.export().unwrap();
        rst_pin
            .set_direction(Direction::Out)
            .expect("LCD Reset pin problem");
        let mut disp = ST7920::<Spidev, Pin, Pin>::new(spi, rst_pin, None, false);
        disp.init(&mut delay).expect("could not init display");
        disp.clear(&mut delay).expect("could not clear display");

        loop {
            let cmd_ev = state_changes_receiver.try_recv();
            if cmd_ev.is_err() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            let cmd_ev = cmd_ev.unwrap();
            trace!("Command event received: {:?}", cmd_ev);
            match cmd_ev {
                CommandEvent::PlayerStatusChanged(stat) => {
                    draw_player_status(&mut disp, &mut delay, stat);
                }
                CommandEvent::StreamerStatusChanged(sstatus) => {
                    draw_streamer_status(&mut disp, &mut delay, sstatus);
                }
                _ => {}
            }
        }
    });
}

fn draw_streamer_status(
    disp: &mut ST7920<Spidev, Pin, Pin>,
    delay: &mut dyn DelayUs<u32>,
    status: StreamerStatus,
) {
    disp.clear_buffer_region(1, 1, 100, 12, delay);
    //1. player name
    Text::new(
        format!(
            "P:{:?} O:{:?} ",
            status.source_player, status.selected_audio_output
        )
        .as_str(),
        Point::new(1, 10),
        MonoTextStyle::new(&FONT_5X8, BinaryColor::On),
    )
    .draw(disp)
    .expect("Failed to draw text");
    disp.flush_region(1, 1, 100, 12, delay)
        .expect("Failed to flush!");
}

fn draw_player_status(
    disp: &mut ST7920<Spidev, Pin, Pin>,
    delay: &mut dyn DelayUs<u32>,
    status: PlayerStatus,
) {
    //4. song
    let name = status.song_info_string();
    let mut title = "".to_string();
    if let Some(name) = name {
        const MAX_LEN: usize = 76;
        if name.len() > MAX_LEN {
            title = unidecode(&name[0..MAX_LEN - 1]);
        } else {
            title = unidecode(name.as_str());
        }
        let rows = title.len() / 20;
        for i in 0..rows {
            title.insert((i + 1) * 19, '\n');
        }
    }
    trace!("Title length: {} / title: {}", title.len(), title);

    disp.clear_buffer_region(1, 12, 120, 40, delay)
        .expect("Error");

    let t = Text::new(
        title.as_str(),
        Point::new(1, 22),
        MonoTextStyle::new(&FONT_6X12, BinaryColor::On),
    );
    t.draw(disp).expect("Failed to draw player status");
    disp.flush_region(1, 12, 120, 40, delay).unwrap();
}
