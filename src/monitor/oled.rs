use std::time::Duration;

use tokio::sync::broadcast::Receiver;

use crate::common::{CommandEvent, PlayerStatus};
use crate::mcu::gpio::GPIO_PIN_OUTPUT_LCD_RST;
use crate::monitor::st7920::ST7920;
use embedded_graphics::fonts::*;
use embedded_graphics::style::TextStyle;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};

use linux_embedded_hal::spidev::SpiModeFlags;
use linux_embedded_hal::spidev::SpidevOptions;
use linux_embedded_hal::sysfs_gpio::Direction;
use linux_embedded_hal::Spidev;
use linux_embedded_hal::{Delay, Pin};
use unidecode::unidecode;

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

        let _text_large = TextStyleBuilder::new(Font24x32)
            .text_color(BinaryColor::On)
            .build();
        let text_medium = TextStyleBuilder::new(Font12x16)
            .text_color(BinaryColor::On)
            .build();
        let text_small = TextStyleBuilder::new(Font8x16)
            .text_color(BinaryColor::On)
            .build();
        let text_pico_6x12 = TextStyleBuilder::new(Font6x12)
            .text_color(BinaryColor::On)
            .build();
        let text_pico_6x8 = TextStyleBuilder::new(Font6x8)
            .text_color(BinaryColor::On)
            .build();
        let text_pico_6x6 = TextStyleBuilder::new(Font6x6)
            .text_color(BinaryColor::On)
            .build();
        loop {
            let cmd_ev = state_changes_receiver.try_recv();
            if cmd_ev.is_err() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            let cmd_ev = cmd_ev.unwrap();
            trace!("Command event received: {:?}", cmd_ev);
            match cmd_ev {
                CommandEvent::Error(e) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(&format!("ERROR:\n{:?}", e).as_str(), Point::new(0, 0))
                        .into_styled(text_small)
                        .into_iter()
                        .draw(&mut disp)
                        .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::PlayerChanged(player) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(format!("PLAY->{:?}", player).as_str(), Point::new(0, 20))
                        .into_styled(text_medium)
                        .into_iter()
                        .draw(&mut disp)
                        .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::AudioOutputChanged(out) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(format!("OUT->{:?}", out).as_str(), Point::new(0, 20))
                        .into_styled(text_medium)
                        .into_iter()
                        .draw(&mut disp)
                        .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::FilterChanged(ft) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(&format!("Filter\n->{:?}", ft).as_str(), Point::new(0, 20))
                        .into_styled(text_small)
                        .into_iter()
                        .draw(&mut disp)
                        .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::SoundChanged(no) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(format!("Snd profile->{:?}", no).as_str(), Point::new(0, 20))
                        .into_styled(text_small)
                        .into_iter()
                        .draw(&mut disp)
                        .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::Bussy(msg) => {
                    disp.clear(&mut delay).expect("Error");
                    Text::new(
                        format!("Bussy {:?}", msg.unwrap_or_else(|| String::from(""))).as_str(),
                        Point::new(0, 20),
                    )
                    .into_styled(text_small)
                    .into_iter()
                    .draw(&mut disp)
                    .unwrap();
                    disp.flush(&mut delay).unwrap();
                }
                CommandEvent::PlayerStatusChanged(stat) => {
                    disp.clear(&mut delay).expect("Error");
                    draw_state(
                        &mut disp,
                        text_small,
                        text_pico_6x12,
                        text_pico_6x8,
                        text_pico_6x6,
                        stat,
                    );
                    disp.flush(&mut delay).unwrap();
                }
                _ => {}
            }
        }
    });
}

fn draw_state(
    disp: &mut ST7920<Spidev, Pin, Pin>,
    text_small_8x16: TextStyle<BinaryColor, Font8x16>,
    text_pico_6x12: TextStyle<BinaryColor, Font6x12>,
    text_pico_6x8: TextStyle<BinaryColor, Font6x8>,
    text_pico_6x6: TextStyle<BinaryColor, Font6x6>,
    status: PlayerStatus,
) {
    //1. player name
    // Text::new(
    //     format!("{:?}", stat.source_player).as_str(),
    //     Point::new(0, 0),
    // )
    // .into_styled(text_pico_6x6)
    // .into_iter()
    // .draw(disp)
    // .unwrap();

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

    //4. song
    let name = status.name;
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
    Text::new(title.as_str(), Point::new(0, 12))
        .into_styled(text_pico_6x12)
        .into_iter()
        .draw(disp)
        .unwrap();
}
