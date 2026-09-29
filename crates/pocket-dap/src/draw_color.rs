use embedded_graphics::draw_target::DrawTargetExt;
use embedded_graphics::mono_font::MonoFont;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_7X13, FONT_7X13_BOLD, FONT_9X15, FONT_9X15_BOLD};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle, Triangle};
use embedded_graphics::text::{Baseline, Text};

use crate::app::{App, Screen, StatusCaption};
use crate::lcd::{self, LcdInk, SEG_PANEL};
use crate::text::{ellipsize, format_time};
use crate::theme::{ColorTheme, Face, Layout, Theme};

pub const WIDTH: u32 = 240;
pub const HEIGHT: u32 = 240;
const STATUS_H: i32 = 16;

pub fn draw<D>(target: &mut D, app: &App, theme: &Theme) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let color = &theme.color;
    target.clear(color.background)?;
    draw_status(target, app, theme)?;
    if theme.layout == Layout::Playlist && matches!(app.screen, Screen::NowPlaying) {
        draw_now_playing_playlist(target, app, color, theme.font)?;
        if app.volume_overlay_visible() {
            draw_volume_overlay(target, app, color)?;
        }
        return Ok(());
    }
    match app.screen {
        Screen::NowPlaying if theme.layout == Layout::Mpod => draw_now_playing_mpod(target, app, color, theme.font)?,
        Screen::NowPlaying => draw_now_playing(target, app, color, theme.font, theme.seven_segment)?,
        Screen::Search => draw_search(target, app, theme)?,
        _ => draw_string_list(target, app, theme, &app.list_labels(), app.list_drills())?,
    }
    if let Some(ch) = app.letter_overlay() {
        draw_letter_overlay(target, ch, color)?;
    }
    if app.volume_overlay_visible() {
        draw_volume_overlay(target, app, color)?;
    }
    Ok(())
}

fn draw_letter_overlay<D>(target: &mut D, ch: char, theme: &ColorTheme) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // FONT_9X15_BOLD is 9×15. Default Text baseline is alphabetic, so use Top
    // and center the glyph inside the fill so the letter is not outside the box.
    let label = ch.to_ascii_uppercase().to_string();
    let style = MonoTextStyle::new(&FONT_9X15_BOLD, theme.foreground);
    let glyph_w = 9i32;
    let glyph_h = 15i32;
    let box_w = 32i32;
    let box_h = 32i32;
    let box_x = (WIDTH as i32 - box_w) / 2;
    let box_y = 96;
    let text_x = box_x + (box_w - glyph_w) / 2;
    let text_y = box_y + (box_h - glyph_h) / 2;
    Rectangle::new(Point::new(box_x, box_y), Size::new(box_w as u32, box_h as u32))
        .into_styled(PrimitiveStyle::with_fill(theme.header))
        .draw(target)?;
    Text::with_baseline(&label, Point::new(text_x, text_y), style, Baseline::Top).draw(target)?;
    Ok(())
}

fn draw_status<D>(target: &mut D, app: &App, theme: &Theme) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let color = &theme.color;
    Rectangle::new(Point::zero(), Size::new(WIDTH, STATUS_H as u32))
        .into_styled(PrimitiveStyle::with_fill(color.header))
        .draw(target)?;
    Rectangle::new(Point::new(0, STATUS_H - 1), Size::new(WIDTH, 1))
        .into_styled(PrimitiveStyle::with_fill(color.track))
        .draw(target)?;
    let ink = if app.connected { color.accent } else { color.dim };
    draw_transport(target, app.paused, ink)?;
    let hint = status_hint(app, theme);
    let hint_w = if hint.is_empty() {
        0
    } else {
        hint.chars().count() as i32 * theme.font.px() + 8
    };
    let text_x = 18;
    let text_right = WIDTH as i32 - 4 - hint_w;
    let clip = Rectangle::new(
        Point::new(text_x, 0),
        Size::new((text_right - text_x).max(0) as u32, STATUS_H as u32 - 1),
    );
    let style = text_style(theme.font, false, ink);
    let baseline = 12;
    match app.status_caption() {
        StatusCaption::None => {}
        StatusCaption::Fixed(text) => {
            let width = ((text_right - text_x) / theme.font.px()).max(1) as usize;
            Text::new(&ellipsize(&text, width), Point::new(text_x, baseline), style).draw(target)?;
        }
        StatusCaption::Scroll(text) => {
            let cw = theme.font.px();
            let text_w = text.chars().count() as i32 * cw;
            let gap = 24;
            let period = (text_w + gap).max(1);
            let shift = app.marquee_px() % period;
            let mut clipped = target.clipped(&clip);
            Text::new(&text, Point::new(text_x - shift, baseline), style).draw(&mut clipped)?;
            Text::new(&text, Point::new(text_x - shift + period, baseline), style).draw(&mut clipped)?;
        }
    }
    if !hint.is_empty() {
        let hint_style = text_style(theme.font, false, color.dim);
        Text::new(&hint, Point::new(text_right, baseline), hint_style).draw(target)?;
    }
    Ok(())
}

fn status_hint(app: &App, theme: &Theme) -> String {
    if !app.connected {
        return "NO HOST".to_string();
    }
    if theme.layout == Layout::Playlist && !matches!(app.screen, Screen::NowPlaying) {
        return app.title().to_string();
    }
    app.artists_letter().map(|c| c.to_string()).unwrap_or_default()
}

fn draw_transport<D>(target: &mut D, paused: bool, ink: Rgb565) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    if paused {
        for dx in [4, 10] {
            Rectangle::new(Point::new(dx, 3), Size::new(3, 10))
                .into_styled(PrimitiveStyle::with_fill(ink))
                .draw(target)?;
        }
    } else {
        Triangle::new(Point::new(4, 3), Point::new(4, 13), Point::new(13, 8))
            .into_styled(PrimitiveStyle::with_fill(ink))
            .draw(target)?;
    }
    Ok(())
}

fn draw_search<D>(target: &mut D, app: &App, theme: &Theme) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let color = &theme.color;
    let query_color = if matches!(theme.layout, Layout::Mpod | Layout::Playlist) {
        color.foreground
    } else {
        color.accent
    };
    let q = text_style(theme.font, false, query_color);
    Text::new(&ellipsize(&app.search_query_line(), 32), Point::new(6, 38), q).draw(target)?;
    let items = app.list_labels();
    if items.is_empty() {
        let dim = text_style(theme.font, false, color.dim);
        Text::new("type or encoder + *", Point::new(6, 58), dim).draw(target)?;
        return Ok(());
    }
    draw_string_list_at(target, app, theme, &items, app.list_drills(), 52)
}

fn draw_now_playing<D>(target: &mut D, app: &App, theme: &ColorTheme, face: Face, seven_segment: bool) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let bold = MonoTextStyle::new(&FONT_9X15_BOLD, theme.foreground);
    let body = text_style(face, false, theme.dim);
    let small = text_style(face, false, theme.foreground);
    if app.song.is_some() {
        Text::new(&ellipsize(&app.song_title(), 24), Point::new(10, 56), bold).draw(target)?;
        Text::new(&ellipsize(&app.song_artist(), 28), Point::new(10, 80), body).draw(target)?;
        Text::new(&ellipsize(&app.song_album(), 28), Point::new(10, 98), body).draw(target)?;
        if seven_segment {
            let ink = LcdInk {
                on: theme.foreground,
                ghost: theme.track,
            };
            let elapsed = lcd::mmss(app.elapsed_secs);
            let total = lcd::mmss(app.total_secs);
            lcd::write(target, Point::new(10, 110), &elapsed, SEG_PANEL, ink)?;
            let total_x = WIDTH as i32 - 10 - lcd::width(&total, SEG_PANEL);
            lcd::write(target, Point::new(total_x, 110), &total, SEG_PANEL, ink)?;
        } else {
            let times = format!("{} / {}", format_time(app.elapsed_secs), format_time(app.total_secs));
            Text::new(&times, Point::new(10, 128), small).draw(target)?;
        }
    } else if app.connected {
        Text::new("Nothing playing", Point::new(10, 56), bold).draw(target)?;
    } else {
        Text::new("Waiting for rsplayer", Point::new(10, 56), bold).draw(target)?;
    }

    Rectangle::new(Point::new(10, 140), Size::new(220, 8))
        .into_styled(PrimitiveStyle::with_fill(theme.track))
        .draw(target)?;
    let filled = u32::from(app.progress_percent()) * 220 / 100;
    if filled > 0 {
        Rectangle::new(Point::new(10, 140), Size::new(filled, 8))
            .into_styled(PrimitiveStyle::with_fill(theme.bar))
            .draw(target)?;
    }
    Ok(())
}

fn draw_volume_overlay<D>(target: &mut D, app: &App, theme: &ColorTheme) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Slim capsule HUD: soft plate + thin rounded track filled from the left.
    let bar_w = 176u32;
    let bar_h = 5u32;
    let x = ((WIDTH - bar_w) / 2) as i32;
    let y = 188;
    let corner = Size::new(bar_h / 2, bar_h / 2);

    RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(x - 8, y - 5), Size::new(bar_w + 16, bar_h + 10)),
        Size::new(7, 7),
    )
    .into_styled(PrimitiveStyle::with_fill(theme.header))
    .draw(target)?;

    RoundedRectangle::with_equal_corners(Rectangle::new(Point::new(x, y), Size::new(bar_w, bar_h)), corner)
        .into_styled(PrimitiveStyle::with_fill(theme.track))
        .draw(target)?;

    let fill_w = bar_w * u32::from(app.volume_percent()) / 100;
    if fill_w > 0 {
        let r = (fill_w / 2).min(bar_h / 2).max(1);
        RoundedRectangle::with_equal_corners(Rectangle::new(Point::new(x, y), Size::new(fill_w, bar_h)), Size::new(r, r))
            .into_styled(PrimitiveStyle::with_fill(theme.bar))
            .draw(target)?;
    }
    Ok(())
}

fn draw_now_playing_mpod<D>(target: &mut D, app: &App, theme: &ColorTheme, face: Face) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let bold = MonoTextStyle::new(&FONT_9X15_BOLD, theme.foreground);
    let body = text_style(face, false, theme.dim);
    if app.song.is_some() {
        let title = ellipsize(&app.song_title(), 22);
        Text::new(&title, Point::new(center_x(&title, 9), 78), bold).draw(target)?;
        let artist = ellipsize(&app.song_artist(), 28);
        Text::new(&artist, Point::new(center_x(&artist, face.px()), 102), body).draw(target)?;
        let album = ellipsize(&app.song_album(), 28);
        Text::new(&album, Point::new(center_x(&album, face.px()), 120), body).draw(target)?;
    } else if app.connected {
        let msg = "Nothing playing";
        Text::new(msg, Point::new(center_x(msg, 9), 78), bold).draw(target)?;
    } else {
        let msg = "Waiting for rsplayer";
        Text::new(msg, Point::new(center_x(msg, 9), 78), bold).draw(target)?;
    }

    let elapsed = format_time(app.elapsed_secs);
    let total = format_time(app.total_secs);
    let small = text_style(face, false, theme.dim);
    let y = 176;
    let cw = face.px();
    Text::new(&elapsed, Point::new(8, y), small).draw(target)?;
    let total_x = WIDTH as i32 - 8 - total.chars().count() as i32 * cw;
    Text::new(&total, Point::new(total_x, y), small).draw(target)?;
    let bar_x = 8 + elapsed.chars().count() as i32 * cw + 8;
    let bar_w = (total_x - 8 - bar_x).max(0) as u32;
    let bar_y = y - 10;
    Rectangle::new(Point::new(bar_x, bar_y), Size::new(bar_w, 6))
        .into_styled(PrimitiveStyle::with_fill(theme.track))
        .draw(target)?;
    let filled = u32::from(app.progress_percent()) * bar_w / 100;
    if filled > 0 {
        Rectangle::new(Point::new(bar_x, bar_y), Size::new(filled, 6))
            .into_styled(PrimitiveStyle::with_fill(theme.bar))
            .draw(target)?;
    }
    Ok(())
}

fn draw_now_playing_playlist<D>(target: &mut D, app: &App, theme: &ColorTheme, face: Face) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, theme.foreground);
    let body = text_style(face, false, theme.dim);
    if app.song.is_some() {
        Text::new(&ellipsize(&app.song_title(), 24), Point::new(10, 56), title_style).draw(target)?;
        Text::new(&ellipsize(&app.song_artist(), 30), Point::new(10, 78), body).draw(target)?;
        Text::new(&ellipsize(&app.song_album(), 30), Point::new(10, 96), body).draw(target)?;
    } else if app.connected {
        Text::new("Nothing playing", Point::new(10, 56), title_style).draw(target)?;
    } else {
        Text::new("Waiting for rsplayer", Point::new(10, 56), title_style).draw(target)?;
    }

    let ink = LcdInk {
        on: theme.foreground,
        ghost: theme.track,
    };
    let elapsed = lcd::mmss(app.elapsed_secs);
    lcd::write(target, Point::new(10, 120), &elapsed, SEG_PANEL, ink)?;
    let total = format_time(app.total_secs);
    let total_x = 10 + lcd::width(&elapsed, SEG_PANEL) + 12;
    Text::new(&total, Point::new(total_x, 142), body).draw(target)?;
    Ok(())
}

fn draw_string_list<D>(target: &mut D, app: &App, theme: &Theme, items: &[String], drill: bool) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_string_list_at(target, app, theme, items, drill, 32)
}

fn draw_string_list_at<D>(target: &mut D, app: &App, theme: &Theme, items: &[String], drill: bool, first_y: i32) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let color = &theme.color;
    let playlist = theme.layout == Layout::Playlist;
    let large = theme.font == Face::Mono9;
    let row_h: i32 = if large {
        22
    } else if playlist {
        16
    } else {
        20
    };
    let visible: usize = if large {
        8
    } else if playlist {
        12
    } else {
        9
    };
    let start = app.cursor.saturating_sub(visible.saturating_sub(1));
    let fg = text_style(theme.font, false, color.foreground);
    let hi = text_style(theme.font, true, color.highlight_fg);
    let rect_h = if large {
        20
    } else if playlist {
        15
    } else {
        18
    };
    let rect_dy = if large {
        16
    } else if playlist {
        12
    } else {
        14
    };
    for (row, (i, item)) in items.iter().enumerate().skip(start).take(visible).enumerate() {
        let y = first_y + (row as i32) * row_h;
        let selected = i == app.cursor && (!app.on_search() || app.search_highlight_results());
        if selected {
            Rectangle::new(Point::new(0, y - rect_dy), Size::new(WIDTH, rect_h))
                .into_styled(PrimitiveStyle::with_fill(color.highlight))
                .draw(target)?;
        }
        let label = if playlist {
            ellipsize(&format!("{}. {}", i + 1, item), 32)
        } else {
            ellipsize(item, if drill { 26 } else { 28 })
        };
        let style = if selected { hi } else { fg };
        Text::new(&label, Point::new(8, y), style).draw(target)?;
        if drill && !playlist {
            Text::new(">", Point::new(224, y), style).draw(target)?;
        }
    }
    Ok(())
}

fn center_x(text: &str, char_w: i32) -> i32 {
    let px = text.chars().count() as i32 * char_w;
    (WIDTH as i32 - px) / 2
}

fn text_style<C: PixelColor>(face: Face, bold: bool, color: C) -> MonoTextStyle<'static, C> {
    MonoTextStyle::new(face_font(face, bold), color)
}

fn face_font(face: Face, bold: bool) -> &'static MonoFont<'static> {
    match (face, bold) {
        (Face::Mono7, false) => &FONT_7X13,
        (Face::Mono7, true) => &FONT_7X13_BOLD,
        (Face::Mono9, false) => &FONT_9X15,
        (Face::Mono9, true) => &FONT_9X15_BOLD,
    }
}
