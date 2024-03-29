use api_models::state::StateChangeEvent;
use rsplayer_config::ArcConfiguration;
use tokio::sync::broadcast::Receiver;

pub async fn write(state_changes_rx: Receiver<StateChangeEvent>, config: ArcConfiguration) {
    let settings = config.get_settings();
    if settings.oled_settings.enabled {
        hw_oled::write(state_changes_rx, settings.oled_settings).await;
    } else {
        crate::common::logging_receiver_future(state_changes_rx).await;
    }
}

mod hw_oled {
    use super::{Receiver, StateChangeEvent};
    use crate::mcu::gpio::{get_output_pin_handle, GPIO_PIN_OUTPUT_LCD_RST};
    use api_models::{player::Song, settings::OLEDSettings, state::PlayerInfo};
    use embedded_graphics::{
        mono_font::{ascii::FONT_4X6, ascii::FONT_5X8, ascii::FONT_6X12, MonoTextStyle},
        pixelcolor::BinaryColor,
        prelude::*,
        text::Text,
    };

    use log::{debug, error, info};
    use st7920::ST7920;
    use unidecode::unidecode;

    use api_models::state::StreamerState;

    use linux_embedded_hal::CdevPin;

    use linux_embedded_hal::spidev::{SpiModeFlags, SpidevOptions};
    use linux_embedded_hal::{Delay, SpidevDevice};

    pub async fn write(mut state_changes_rx: Receiver<StateChangeEvent>, oled_settings: OLEDSettings) {
        let mut delay = Delay;
        if let Ok(mut spi) = SpidevDevice::open(oled_settings.spi_device_path) {
            info!("Start OLED writer thread.");
            let options = SpidevOptions::new()
                .bits_per_word(8)
                .max_speed_hz(800_000)
                .mode(SpiModeFlags::SPI_CS_HIGH)
                .build();
            spi.configure(&options).expect("error configuring SPI");
            let rst_pin = get_output_pin_handle(GPIO_PIN_OUTPUT_LCD_RST).unwrap();
            let rst_pin = CdevPin::new(rst_pin).expect("LCD Reset pin problem");
            let mut disp = ST7920::<SpidevDevice, CdevPin, CdevPin>::new(spi, rst_pin, None, false);
            disp.init(&mut delay).expect("could not init display");
            disp.clear(&mut delay).expect("could not clear display");
            loop {
                let cmd_ev = state_changes_rx.recv().await;
                debug!("Command event received: {:?}", cmd_ev);
                match cmd_ev {
                    Ok(StateChangeEvent::CurrentSongEvent(stat)) => {
                        draw_track_info(&mut disp, &mut delay, &stat);
                    }
                    Ok(StateChangeEvent::StreamerStateEvent(sstatus)) => {
                        draw_streamer_info(&mut disp, &mut delay, &sstatus);
                    }
                    Ok(StateChangeEvent::PlayerInfoEvent(pinfo)) => {
                        draw_player_info(&mut disp, &mut delay, &pinfo);
                    }
                    _ => {}
                }
            }
        } else {
            error!("Failed to configure OLED display");
            crate::common::no_op_future().await;
        }
    }

    fn draw_streamer_info(
        disp: &mut ST7920<SpidevDevice, CdevPin, CdevPin>,
        delay: &mut Delay,
        status: &StreamerState,
    ) {
        _ = disp.clear_buffer_region(1, 1, 120, 12);
        //1. player name
        Text::new(
            format!(
                "P:{:?}|O:{:?}|V:{:?}",
                "RSP", status.selected_audio_output, status.volume_state.current
            )
            .as_str(),
            Point::new(1, 10),
            MonoTextStyle::new(&FONT_5X8, BinaryColor::On),
        )
        .draw(disp)
        .expect("Failed to draw text");
        disp.flush_region(1, 1, 120, 12, delay).expect("Failed to flush!");
    }

    fn draw_track_info(disp: &mut ST7920<SpidevDevice, CdevPin, CdevPin>, delay: &mut Delay, status: &Song) {
        //4. song
        let name = status.info_string();
        let mut title = String::new();
        if let Some(name) = name {
            const MAX_LEN: usize = 76;
            if name.len() > MAX_LEN {
                // todo: it is panicking here for some chars  (panicked at 'byte index 75 is not a char boundary; it is inside 'š' (bytes 74..76) of `Đorđe Balašević-Marim ja)
                title = unidecode(&name[0..MAX_LEN - 1]);
            } else {
                title = unidecode(name.as_str());
            }
            let rows = title.len() / 22;
            for i in 0..rows {
                title.insert((i + 1) * 21, '\n');
            }
        }
        debug!("Title length: {} / title: {}", title.len(), title);

        disp.clear_buffer_region(1, 12, 120, 40).expect("Error");

        let t = Text::new(
            title.as_str(),
            Point::new(1, 22),
            MonoTextStyle::new(&FONT_6X12, BinaryColor::On),
        );
        t.draw(disp).expect("Failed to draw player status");
        disp.flush_region(1, 12, 120, 40, delay).unwrap();
    }

    fn draw_player_info(
        disp: &mut ST7920<SpidevDevice, CdevPin, CdevPin>,
        delay: &mut Delay,
        player_info: &PlayerInfo,
    ) {
        _ = disp.clear_buffer_region(1, 50, 120, 12);
        //1. player name
        Text::new(
            format!(
                "F:{:?}|B:{:?}|C:{:?}",
                // player_info.time.0.as_secs(),
                // player_info.time.1.as_secs(),
                player_info.audio_format_rate.unwrap_or_default(),
                player_info.audio_format_bit.unwrap_or_default(),
                player_info.audio_format_channels.unwrap_or_default(),
                // player_info
                //     .state
                //     .map_or(String::new(), |s| if s == PlayerState::PLAYING {
                //         "|>".to_string()
                //     } else {
                //         String::new()
                //     }),
                // player_info
                //     .random
                //     .map_or(String::new(), |r| if r { "|rnd".to_string() } else { String::new() })
            )
            .as_str(),
            Point::new(1, 60),
            MonoTextStyle::new(&FONT_4X6, BinaryColor::On),
        )
        .draw(disp)
        .expect("Failed to draw text");
        disp.flush_region(1, 50, 120, 12, delay).expect("Failed to flush!");
    }
}
