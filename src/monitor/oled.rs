use std::time::Duration;

use embedded_graphics::{
    mono_font::{ascii::FONT_5X8, ascii::FONT_6X12, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
    text::Text,
};
use embedded_hal::blocking::delay::DelayUs;
use embedded_hal::prelude::{_embedded_hal_blocking_delay_DelayUs, _embedded_hal_serial_Write};
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

        // let _text_large = TextStyleBuilder::new(Font24x32)
        //     .text_color(BinaryColor::On)
        //     .build();
        // let text_medium = TextStyleBuilder::new(Font12x16)
        //     .text_color(BinaryColor::On)
        //     .build();
        // let text_small = TextStyleBuilder::new(Font8x16)
        //     .text_color(BinaryColor::On)
        //     .build();
        // let text_pico_6x12 = TextStyleBuilder::new(Font6x12)
        //     .text_color(BinaryColor::On)
        //     .build();
        // let text_pico_6x8 = TextStyleBuilder::new(Font6x8)
        //     .text_color(BinaryColor::On)
        //     .build();
        // let text_pico_6x6 = TextStyleBuilder::new(Font6x6)
        //     .text_color(BinaryColor::On)
        //     .build();
        loop {
            let cmd_ev = state_changes_receiver.try_recv();
            if cmd_ev.is_err() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            let cmd_ev = cmd_ev.unwrap();
            trace!("Command event received: {:?}", cmd_ev);
            match cmd_ev {
                // CommandEvent::Error(e) => {
                //     // disp.clear(&mut delay).expect("Error");
                //     Text::new(&format!("ERROR:\n{:?}", e).as_str(), Point::new(0, 0))
                //         .into_styled(text_small)
                //         .into_iter()
                //         .draw(&mut disp)
                //         .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
                // CommandEvent::PlayerChanged(player) => {
                //     // disp.clear(&mut delay).expect("Error");
                //     Text::new(format!("PLAY->{:?}", player).as_str(), Point::new(0, 20))
                //         .into_styled(text_medium)
                //         .into_iter()
                //         .draw(&mut disp)
                //         .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
                // CommandEvent::AudioOutputChanged(out) => {
                //     // disp.clear(&mut delay).expect("Error");
                //     Text::new(format!("OUT->{:?}", out).as_str(), Point::new(0, 20))
                //         .into_styled(text_medium)
                //         .into_iter()
                //         .draw(&mut disp)
                //         .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
                // CommandEvent::FilterChanged(ft) => {
                //     //disp.clear(&mut delay).expect("Error");
                //     Text::new(&format!("Filter\n->{:?}", ft).as_str(), Point::new(0, 20))
                //         .into_styled(text_small)
                //         .into_iter()
                //         .draw(&mut disp)
                //         .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
                // CommandEvent::SoundChanged(no) => {
                //     //disp.clear(&mut delay).expect("Error");
                //     Text::new(format!("Snd profile->{:?}", no).as_str(), Point::new(0, 20))
                //         .into_styled(text_small)
                //         .into_iter()
                //         .draw(&mut disp)
                //         .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
                // CommandEvent::Busy(msg) => {
                //     //disp.clear(&mut delay).expect("Error");
                //     Text::new(
                //         format!("Busy {:?}", msg.unwrap_or_else(|| String::from(""))).as_str(),
                //         Point::new(0, 20),
                //     )
                //     .into_styled(text_small)
                //     .into_iter()
                //     .draw(&mut disp)
                //     .unwrap();
                //     disp.flush(&mut delay).unwrap();
                // }
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
            "P:{:?} OUT:{:?} ",
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
    // //3. selected audio output
    // Text::new(
    //     format!("->{:?}", stat.selected_audio_out).as_str(),
    //     Point::new(18, 0),
    // )
    // .into_styled(text_pico_6x6)
    // .into_iter()
    // .draw(disp)
    // .unwrap();

    // let dac_status = stat.dac_status.unwrap();
    // //2. volume level
    // Text::new(
    //     format!("@{:?}", &dac_status.volume_level).as_str(),
    //     Point::new(50, 0),
    // )
    // .into_styled(text_pico_6x6)
    // .into_iter()
    // .draw(disp)
    // .unwrap();

    // Text::new(
    //     &format!("F:{:?}", &dac_status.filter).as_str()[0..6],
    //     Point::new(70, 0),
    // )
    // .into_styled(text_pico_6x6)
    // .into_iter()
    // .draw(disp)
    // .unwrap();
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
