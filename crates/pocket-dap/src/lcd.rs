//! Optional 7-segment digits (LCD-style: ghost unlit segments, not LED).

use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

const SEG_A: u8 = 1 << 0;
const SEG_B: u8 = 1 << 1;
const SEG_C: u8 = 1 << 2;
const SEG_D: u8 = 1 << 3;
const SEG_E: u8 = 1 << 4;
const SEG_F: u8 = 1 << 5;
const SEG_G: u8 = 1 << 6;
const SEG_ALL: u8 = SEG_A | SEG_B | SEG_C | SEG_D | SEG_E | SEG_F | SEG_G;

#[derive(Clone, Copy)]
pub struct LcdInk<C> {
    pub on: C,
    pub ghost: C,
}

#[derive(Clone, Copy)]
pub struct SegSize {
    pub w: i32,
    pub h: i32,
    pub t: i32,
    pub gap: i32,
}

pub const SEG_PANEL: SegSize = SegSize {
    w: 13,
    h: 23,
    t: 2,
    gap: 3,
};

pub fn mmss(secs: u32) -> [char; 5] {
    let m = (secs / 60).min(99);
    let s = secs % 60;
    [
        char::from(b'0' + (m / 10) as u8),
        char::from(b'0' + (m % 10) as u8),
        ':',
        char::from(b'0' + (s / 10) as u8),
        char::from(b'0' + (s % 10) as u8),
    ]
}

pub fn write<D>(target: &mut D, origin: Point, chars: &[char], size: SegSize, ink: LcdInk<D::Color>) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: Copy,
{
    let mut x = origin.x;
    for ch in chars {
        if *ch == ':' {
            colon(target, Point::new(x, origin.y), size, ink)?;
            x += size.t * 2 + size.gap;
        } else {
            digit(target, Point::new(x, origin.y), *ch, size, ink)?;
            x += size.w + size.gap;
        }
    }
    Ok(())
}

pub fn width(chars: &[char], size: SegSize) -> i32 {
    let mut w = 0;
    for (i, ch) in chars.iter().enumerate() {
        if i > 0 {
            w += size.gap;
        }
        w += if *ch == ':' { size.t * 2 } else { size.w };
    }
    w
}

fn pattern(ch: char) -> u8 {
    match ch {
        '0' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_E | SEG_F,
        '1' => SEG_B | SEG_C,
        '2' => SEG_A | SEG_B | SEG_G | SEG_E | SEG_D,
        '3' => SEG_A | SEG_B | SEG_G | SEG_C | SEG_D,
        '4' => SEG_F | SEG_G | SEG_B | SEG_C,
        '5' => SEG_A | SEG_F | SEG_G | SEG_C | SEG_D,
        '6' => SEG_A | SEG_F | SEG_G | SEG_E | SEG_C | SEG_D,
        '7' => SEG_A | SEG_B | SEG_C,
        '8' => SEG_ALL,
        '9' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_F | SEG_G,
        '-' => SEG_G,
        _ => 0,
    }
}

fn digit<D>(target: &mut D, origin: Point, ch: char, size: SegSize, ink: LcdInk<D::Color>) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: Copy,
{
    paint_digit(target, origin, size, SEG_ALL, ink.ghost)?;
    paint_digit(target, origin, size, pattern(ch), ink.on)?;
    Ok(())
}

fn paint_digit<D>(target: &mut D, origin: Point, size: SegSize, mask: u8, color: D::Color) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: Copy,
{
    if mask == 0 {
        return Ok(());
    }
    let x = origin.x;
    let y = origin.y;
    let w = size.w;
    let h = size.h;
    let t = size.t;
    let mid = y + (h - t) / 2;
    let inner = (w - 2 * t).max(1) as u32;
    let t_u = t as u32;
    let half = ((h / 2) - t).max(1) as u32;
    let fill = PrimitiveStyle::with_fill(color);
    if mask & SEG_A != 0 {
        Rectangle::new(Point::new(x + t, y), Size::new(inner, t_u))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_G != 0 {
        Rectangle::new(Point::new(x + t, mid), Size::new(inner, t_u))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_D != 0 {
        Rectangle::new(Point::new(x + t, y + h - t), Size::new(inner, t_u))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_F != 0 {
        Rectangle::new(Point::new(x, y + t), Size::new(t_u, half))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_B != 0 {
        Rectangle::new(Point::new(x + w - t, y + t), Size::new(t_u, half))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_E != 0 {
        Rectangle::new(Point::new(x, mid + t), Size::new(t_u, half))
            .into_styled(fill)
            .draw(target)?;
    }
    if mask & SEG_C != 0 {
        Rectangle::new(Point::new(x + w - t, mid + t), Size::new(t_u, half))
            .into_styled(fill)
            .draw(target)?;
    }
    Ok(())
}

fn colon<D>(target: &mut D, origin: Point, size: SegSize, ink: LcdInk<D::Color>) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: Copy,
{
    let t = size.t.max(1);
    let blob = Size::new(t as u32, t as u32);
    let x = origin.x;
    let y1 = origin.y + size.h / 3;
    let y2 = origin.y + (size.h * 2) / 3;
    Rectangle::new(Point::new(x, y1), blob)
        .into_styled(PrimitiveStyle::with_fill(ink.on))
        .draw(target)?;
    Rectangle::new(Point::new(x, y2), blob)
        .into_styled(PrimitiveStyle::with_fill(ink.on))
        .draw(target)?;
    Ok(())
}
