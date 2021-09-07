use std::sync::mpsc::Receiver;
use std::thread;

use embedded_graphics::fonts::{Font12x16, Font24x32, Font6x12, Font8x16};
use embedded_graphics::style::TextStyle;
use embedded_graphics::{
    fonts::Text, pixelcolor::BinaryColor, prelude::*, style::TextStyleBuilder,
};
use linux_embedded_hal::I2cdev;
use sh1106::interface::I2cInterface;
use sh1106::{mode::GraphicsMode, Builder};
use unidecode::unidecode;

use crate::common::{CommandEvent, DPlayStatus};

pub fn start(state_changes_rx: Receiver<CommandEvent>) {
    thread::Builder::new()
        .name("oled-command-events-thread".to_string())
        .spawn(move || {
            let i2c = I2cdev::new("/dev/i2c-1").unwrap();
            let mut disp: GraphicsMode<_> = Builder::new().connect_i2c(i2c).into();
            disp.init().unwrap();
            disp.flush().unwrap();
            let text_large = TextStyleBuilder::new(Font24x32)
                .text_color(BinaryColor::On)
                .background_color(BinaryColor::Off)
                .build();
            let text_medium = TextStyleBuilder::new(Font12x16)
                .text_color(BinaryColor::On)
                .background_color(BinaryColor::Off)
                .build();
            let text_small = TextStyleBuilder::new(Font8x16)
                .text_color(BinaryColor::On)
                .background_color(BinaryColor::Off)
                .build();
            let text_pico = TextStyleBuilder::new(Font6x12)
                .text_color(BinaryColor::On)
                .background_color(BinaryColor::Off)
                .build();

            for cmd_ev in state_changes_rx {
                trace!("Command event received: {:?}", cmd_ev);
                disp.clear();
                match cmd_ev {
                    CommandEvent::Error(e) => {
                        Text::new(format!("Error: {:?}", e).as_str(), Point::new(0, 20))
                            .into_styled(text_medium)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                    }
                    CommandEvent::PlayerChanged(player) => {
                        Text::new(format!("{:?}", player).as_str(), Point::new(0, 20))
                            .into_styled(text_medium)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                    }
                    CommandEvent::VolumeChanged(vol) => {
                        Text::new(format!("{}", vol).as_str(), Point::new(20, 20))
                            .into_styled(text_large)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                    }
                    CommandEvent::Playing => {
                        Text::new("|>", Point::new(30, 20))
                            .into_styled(text_large)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                        disp.flush().unwrap();
                    }
                    CommandEvent::Paused => {
                        draw_paused(&mut disp, text_large);
                    }
                    CommandEvent::SwitchedToNextTrack => {
                        Text::new(">>", Point::new(30, 20))
                            .into_styled(text_large)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                    }
                    CommandEvent::SwitchedToPrevTrack => {
                        Text::new("<<", Point::new(30, 20))
                            .into_styled(text_large)
                            .into_iter()
                            .draw(&mut disp)
                            .unwrap();
                    }
                    CommandEvent::StateChanged(stat) => {
                        if stat.playing {
                            draw_state(&mut disp, text_small, text_pico, stat);
                        } else {
                            draw_paused(&mut disp, text_large);
                        }
                    }
                    _ => {}
                }
                disp.flush().unwrap();
            }
        })
        .unwrap();
}

fn draw_paused(
    disp: &mut GraphicsMode<I2cInterface<I2cdev>>,
    text_large: TextStyle<BinaryColor, Font24x32>,
) {
    Text::new("||", Point::new(30, 20))
        .into_styled(text_large)
        .into_iter()
        .draw(disp)
        .unwrap();
}

fn draw_state(
    disp: &mut GraphicsMode<I2cInterface<I2cdev>>,
    text_small: TextStyle<BinaryColor, Font8x16>,
    text_pico: TextStyle<BinaryColor, Font6x12>,
    stat: DPlayStatus,
) {
    disp.clear();
    //1. player name
    let status = stat.player_status.unwrap();
    Text::new(
        format!("{:?}", status.source_player).as_str(),
        Point::new(0, 0),
    )
    .into_styled(text_small)
    .into_iter()
    .draw(disp)
    .unwrap();

    //2. volume level
    Text::new(
        format!(" - {:?}", stat.dac_status.unwrap().volume_level).as_str(),
        Point::new(64, 0),
    )
    .into_styled(text_small)
    .into_iter()
    .draw(disp)
    .unwrap();

    //3. song
    let mut title = unidecode(status.song.as_str());
    let rows = title.len() / 20;
    for i in 0..rows {
        title.insert((i + 1) * 19, '\n');
    }
    Text::new(title.as_str(), Point::new(0, 17))
        .into_styled(text_pico)
        .into_iter()
        .draw(disp)
        .unwrap();
}
