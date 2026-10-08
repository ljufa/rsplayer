use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_6X13_BOLD};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::app::App;
use crate::text::{ellipsize, format_time};

pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 64;

pub fn draw<D>(target: &mut D, app: &App) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    target.clear(BinaryColor::Off)?;
    match app.screen {
        crate::app::Screen::NowPlaying => draw_now_playing(target, app)?,
        _ => draw_list(target, app)?,
    }
    Ok(())
}

fn draw_now_playing<D>(target: &mut D, app: &App) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let bold = MonoTextStyle::new(&FONT_6X13_BOLD, BinaryColor::On);
    let body = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let header = if app.connected { app.title() } else { "NO HOST" };
    Text::new(header, Point::new(0, 10), body).draw(target)?;
    if app.song.is_some() {
        Text::new(&ellipsize(&app.song_title(), 21), Point::new(0, 24), bold).draw(target)?;
        Text::new(&ellipsize(&app.song_artist(), 21), Point::new(0, 36), body).draw(target)?;
        let times = format!("{} / {}", format_time(app.elapsed_secs), format_time(app.total_secs));
        Text::new(&times, Point::new(0, 50), body).draw(target)?;
    } else if app.connected {
        Text::new("Nothing playing", Point::new(0, 24), bold).draw(target)?;
    } else {
        Text::new("Waiting..", Point::new(0, 24), bold).draw(target)?;
    }
    let filled = u32::from(app.progress_percent()) * WIDTH / 100;
    if filled > 0 {
        Rectangle::new(Point::new(0, 54), Size::new(filled, 3))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(target)?;
    }
    let vol = u32::from(app.volume_percent()) * WIDTH / 100;
    if vol > 0 {
        Rectangle::new(Point::new(0, 61), Size::new(vol, 3))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(target)?;
    }
    Ok(())
}

fn draw_list<D>(target: &mut D, app: &App) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let body = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let header = if app.on_search() {
        ellipsize(&app.search_query_line(), 21)
    } else if let Some(letter) = app.artists_letter() {
        format!("{} {letter}", app.title())
    } else {
        app.title().to_string()
    };
    Text::new(&header, Point::new(0, 10), body).draw(target)?;
    let items = app.list_labels();
    let visible = 5usize;
    let start = app.cursor.saturating_sub(visible.saturating_sub(1));
    let on = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let off = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
    let first_y = 22;
    for (row, (i, item)) in items.iter().enumerate().skip(start).take(visible).enumerate() {
        let y = first_y + (row as i32) * 10;
        let selected = i == app.cursor && (!app.on_search() || app.search_highlight_results());
        if selected {
            Rectangle::new(Point::new(0, y - 8), Size::new(WIDTH, 10))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(target)?;
        }
        let style = if selected { off } else { on };
        Text::new(&ellipsize(item, 21), Point::new(1, y), style).draw(target)?;
    }
    Ok(())
}
