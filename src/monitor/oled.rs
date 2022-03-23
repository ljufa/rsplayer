use api_models::player::StatusChangeEvent;
use cfg_if::cfg_if;
use tokio::sync::broadcast::Receiver;

use crate::common;

pub async fn write(mut state_changes_rx: Receiver<StatusChangeEvent>) {
    cfg_if! {
        if #[cfg(feature="hw_oled")] {
            hw_oled::write(state_changes_rx)
        } else{
            common::logging_receiver_future(state_changes_rx).await;
        }
    }
}

#[cfg(feature = "hw_oled")]
mod hw_oled {
    use crate::mcu::gpio::GPIO_PIN_OUTPUT_LCD_RST;
    use crate::monitor::myst7920::ST7920;
    use embedded_graphics::{
        mono_font::{ascii::FONT_4X6, ascii::FONT_5X8, ascii::FONT_6X12, MonoTextStyle},
        pixelcolor::BinaryColor,
        prelude::*,
        text::Text,
    };
    use embedded_hal::blocking::delay::DelayUs;
    use unidecode::unidecode;

    use linux_embedded_hal::spidev::SpiModeFlags;
    use linux_embedded_hal::spidev::SpidevOptions;
    use linux_embedded_hal::sysfs_gpio::Direction;
    use linux_embedded_hal::Spidev;
    use linux_embedded_hal::{Delay, Pin};

    pub async fn write(mut state_changes_rx: Receiver<StatusChangeEvent>) {
        info!("Start OLED writer thread.");
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
            let cmd_ev = state_changes_rx.recv().await;
            trace!("Command event received: {:?}", cmd_ev);
            match cmd_ev {
                Ok(StatusChangeEvent::CurrentTrackInfoChanged(stat)) => {
                    draw_track_info(&mut disp, &mut delay, stat);
                }
                Ok(StatusChangeEvent::StreamerStatusChanged(sstatus)) => {
                    draw_streamer_info(&mut disp, &mut delay, sstatus);
                }
                Ok(StatusChangeEvent::PlayerInfoChanged(pinfo)) => {
                    draw_player_info(&mut disp, &mut delay, pinfo);
                }
                _ => {}
            }
        }
    }

    fn draw_streamer_info(
        disp: &mut ST7920<Spidev, Pin, Pin>,
        delay: &mut dyn DelayUs<u32>,
        status: StreamerStatus,
    ) {
        disp.clear_buffer_region(1, 1, 120, 12, delay);
        //1. player name
        Text::new(
            format!(
                "P:{:?}|O:{:?}|V:{:?}",
                status.source_player, status.selected_audio_output, status.dac_status.volume
            )
            .as_str(),
            Point::new(1, 10),
            MonoTextStyle::new(&FONT_5X8, BinaryColor::On),
        )
        .draw(disp)
        .expect("Failed to draw text");
        disp.flush_region(1, 1, 120, 12, delay)
            .expect("Failed to flush!");
    }

    fn draw_track_info(
        disp: &mut ST7920<Spidev, Pin, Pin>,
        delay: &mut dyn DelayUs<u32>,
        status: CurrentTrackInfo,
    ) {
        //4. song
        let name = status.info_string();
        let mut title = "".to_string();
        if let Some(name) = name {
            const MAX_LEN: usize = 76;
            if name.len() > MAX_LEN {
                title = unidecode(&name[0..MAX_LEN - 1]);
            } else {
                title = unidecode(name.as_str());
            }
            let rows = title.len() / 22;
            for i in 0..rows {
                title.insert((i + 1) * 21, '\n');
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

    fn draw_player_info(
        disp: &mut ST7920<Spidev, Pin, Pin>,
        delay: &mut dyn DelayUs<u32>,
        player_info: PlayerInfo,
    ) {
        disp.clear_buffer_region(1, 50, 120, 12, delay);
        //1. player name
        Text::new(
            format!(
                "T:{:?}/{:?}|F:{:?}|B:{:?}|C:{:?}{}{}",
                player_info.time.0.as_secs(),
                player_info.time.1.as_secs(),
                player_info.audio_format_rate.unwrap_or_default(),
                player_info.audio_format_bit.unwrap_or_default(),
                player_info.audio_format_channels.unwrap_or_default(),
                player_info
                    .state
                    .map_or("".to_string(), |s| if s == PlayerState::PLAYING {
                        "|>".to_string()
                    } else {
                        "".to_string()
                    }),
                player_info.random.map_or("".to_string(), |r| if r {
                    "|rnd".to_string()
                } else {
                    "".to_string()
                })
            )
            .as_str(),
            Point::new(1, 60),
            MonoTextStyle::new(&FONT_4X6, BinaryColor::On),
        )
        .draw(disp)
        .expect("Failed to draw text");
        disp.flush_region(1, 50, 120, 12, delay)
            .expect("Failed to flush!");
    }
}
