//! X11/HDMI window UI: panel framebuffer plus optional chassis chrome.
//!
//! Default path for the binary (`--backend window`). `--backend spi` must not
//! open a window (not implemented yet). Chassis chrome (click wheel, GPIO
//! volume stand-ins) is drawn only with `--show-controls`. Encoder steps come
//! from dragging the ring, the mouse wheel, or `[` `]`.

use std::convert::Infallible;
use std::f32::consts::PI;

use embedded_graphics::pixelcolor::{BinaryColor, Rgb565};
use embedded_graphics::prelude::IntoStorage;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Scale, Window, WindowOptions};

use crate::app::App;
use crate::draw_bw;
use crate::draw_color;
use crate::fb::Buffer;
use crate::host::{self, HostEvent};
use crate::input::{DisplayKind, Input};
use crate::theme::{Behavior, Theme};

/// Options for the X11/HDMI window path.
pub struct WindowConfig {
    pub ws_url: String,
    pub display: DisplayKind,
    pub show_controls: bool,
    pub full_screen: bool,
    /// Largest window that fits the monitor (`Scale::FitScreen`).
    pub fit_screen: bool,
    pub theme: Theme,
}

const _: () = {
    assert!(draw_color::WIDTH == 240);
    assert!(draw_color::HEIGHT == 240);
    assert!(draw_bw::WIDTH == 128);
    assert!(draw_bw::HEIGHT == 64);
};

const SCREEN_FRAME: usize = 8;
const CHASSIS_H: usize = 268;
const WHEEL_OUTER: i32 = 118;
const CTR_R: i32 = 42;
const ICON_R: i32 = 80;
const SIDE_R: i32 = 16;
const SIDE_X: i32 = 22;
const SIDE_SPAN: i32 = 26;
const CLUSTER_PAD: i32 = 16;
/// One encoder detent, in radians (~18°).
const WHEEL_DETENT: f32 = PI / 10.0;

struct Hit {
    cx: i32,
    cy: i32,
    r: i32,
    input: Input,
    icon: Icon,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Menu,
    SkipBack,
    SkipFwd,
    PlayPause,
    Back,
    Plus,
    Minus,
}

struct Chassis {
    vols: Vec<Hit>,
    extras: Vec<Hit>,
    bezel_cx: i32,
    bezel_cy: i32,
}

struct Wheel {
    angle: f32,
    drag: Option<WheelDrag>,
}

struct WheelDrag {
    last: f32,
    accum: f32,
    sector: Option<Input>,
    rotated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hover {
    None,
    Center,
    Sector(Input),
    Vol(usize),
    Extra(usize),
}

struct FrameView {
    hover: Hover,
    wheel_angle: f32,
}

struct Win {
    panel_w: usize,
    panel_h: usize,
    win_w: usize,
    win_h: usize,
}

pub fn run_window(cfg: WindowConfig) -> Result<(), minifb::Error> {
    let WindowConfig {
        ws_url,
        display,
        show_controls,
        full_screen,
        fit_screen,
        theme,
    } = cfg;
    match display {
        DisplayKind::St7789 => {
            let fill = theme.color.background;
            let theme_draw = theme.clone();
            run::<Rgb565, 240, 240, 2, _, _>(
                "rsplayer-pocket-dap — ST7789 240×240",
                &ws_url,
                move |fb, app| draw_color::draw(fb, app, &theme_draw),
                rgb565_to_argb,
                fill,
                &theme,
                show_controls,
                full_screen,
                fit_screen,
            )
        }
        DisplayKind::St7567 => run::<BinaryColor, 128, 64, 5, _, _>(
            "rsplayer-pocket-dap — ST7567 128×64",
            &ws_url,
            draw_bw::draw,
            |c| match c {
                BinaryColor::On => 0x00ff_ffff,
                BinaryColor::Off => 0,
            },
            BinaryColor::Off,
            &theme,
            show_controls,
            full_screen,
            fit_screen,
        ),
    }
}

fn rgb565_to_argb(c: Rgb565) -> u32 {
    let raw = c.into_storage();
    let r = ((raw >> 11) & 0x1f) as u32;
    let g = ((raw >> 5) & 0x3f) as u32;
    let b = (raw & 0x1f) as u32;
    let r8 = (r << 3) | (r >> 2);
    let g8 = (g << 2) | (g >> 4);
    let b8 = (b << 3) | (b >> 2);
    (r8 << 16) | (g8 << 8) | b8
}

fn run<C, const W: u32, const H: u32, const SCALE: usize, F, M>(
    title: &str,
    ws_url: &str,
    draw: F,
    map: M,
    fill: C,
    theme: &Theme,
    show_controls: bool,
    full_screen: bool,
    fit_screen: bool,
) -> Result<(), minifb::Error>
where
    C: embedded_graphics::pixelcolor::PixelColor + Copy,
    F: Fn(&mut Buffer<C, W, H>, &App) -> Result<(), Infallible>,
    M: Fn(C) -> u32,
{
    let (cmd_tx, ev_rx) = host::spawn(ws_url.to_string());
    let mut app = App::new();
    app.behavior = theme.behavior;
    let mut fb = Buffer::<C, W, H>::new(fill);

    let panel_w = W as usize * SCALE;
    let panel_h = H as usize * SCALE;
    let (win_w, win_h) = if show_controls {
        (panel_w + SCREEN_FRAME * 2, panel_h + SCREEN_FRAME * 2 + CHASSIS_H)
    } else {
        (panel_w, panel_h)
    };
    let win = Win {
        panel_w,
        panel_h,
        win_w,
        win_h,
    };
    let chassis = if show_controls {
        chassis_layout(panel_h, win.win_w, theme.behavior)
    } else {
        Chassis {
            vols: Vec::new(),
            extras: Vec::new(),
            bezel_cx: -10_000,
            bezel_cy: -10_000,
        }
    };

    let scale = if fit_screen { Scale::FitScreen } else { Scale::X1 };
    let mut window = Window::new(
        title,
        win.win_w,
        win.win_h,
        WindowOptions {
            scale,
            resize: false,
            borderless: full_screen,
            title: !full_screen,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(30);

    let mut mouse_was_down = false;
    let mut menu_hold = MenuHold::default();
    let mut wheel = Wheel { angle: 0.0, drag: None };
    while window.is_open() {
        while let Ok(ev) = ev_rx.try_recv() {
            match ev {
                HostEvent::Connected => {
                    for cmd in app.on_connected() {
                        let _ = cmd_tx.send(cmd);
                    }
                }
                HostEvent::Disconnected => app.on_disconnected(),
                HostEvent::State(state) => {
                    for cmd in app.apply_event(*state) {
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
        }

        app.tick();
        if let Some(input) = poll_input(
            &window,
            &chassis,
            &mut wheel,
            &mut mouse_was_down,
            &mut menu_hold,
            app.on_search(),
            app.on_letter_list(),
            theme.behavior == Behavior::Mpod,
            show_controls,
        ) {
            match app.handle(input) {
                None => break,
                Some(cmds) => {
                    for cmd in cmds {
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
        }

        draw(&mut fb, &app).ok();
        let panel = fb.scaled_argb(SCALE, &map);
        let frame = if show_controls {
            compose_window(
                &panel,
                &win,
                &chassis,
                &FrameView {
                    hover: hover_target(&window, &chassis),
                    wheel_angle: wheel.angle,
                },
                theme,
            )
        } else {
            compose_panel_only(&panel, &win)
        };
        window.update_with_buffer(&frame, win.win_w, win.win_h)?;
    }
    Ok(())
}

fn compose_panel_only(panel: &[u32], win: &Win) -> Vec<u32> {
    let mut out = vec![0u32; win.win_w * win.win_h];
    for y in 0..win.panel_h {
        let src = y * win.panel_w;
        let dst = y * win.win_w;
        out[dst..dst + win.panel_w].copy_from_slice(&panel[src..src + win.panel_w]);
    }
    out
}

fn chassis_layout(panel_h: usize, win_w: usize, behavior: Behavior) -> Chassis {
    let bezel_cx = win_w as i32 / 2;
    let bezel_cy = (panel_h + SCREEN_FRAME * 2) as i32 + CLUSTER_PAD + WHEEL_OUTER;
    let right = win_w as i32 - SIDE_X;
    let vols = vec![
        Hit {
            cx: SIDE_X,
            cy: bezel_cy - SIDE_SPAN,
            r: SIDE_R,
            input: Input::VolUp,
            icon: Icon::Plus,
        },
        Hit {
            cx: SIDE_X,
            cy: bezel_cy + SIDE_SPAN,
            r: SIDE_R,
            input: Input::VolDown,
            icon: Icon::Minus,
        },
    ];
    let extras = if behavior == Behavior::Creative {
        vec![
            Hit {
                cx: right,
                cy: bezel_cy - SIDE_SPAN,
                r: SIDE_R,
                input: Input::Play,
                icon: Icon::PlayPause,
            },
            Hit {
                cx: right,
                cy: bezel_cy + SIDE_SPAN,
                r: SIDE_R,
                input: Input::Back,
                icon: Icon::Back,
            },
        ]
    } else {
        Vec::new()
    };
    Chassis {
        vols,
        extras,
        bezel_cx,
        bezel_cy,
    }
}

fn hover_target(window: &Window, chassis: &Chassis) -> Hover {
    let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) else {
        return Hover::None;
    };
    let px = x as i32;
    let py = y as i32;
    if let Some(i) = chassis.vols.iter().position(|hit| hit.contains(px, py)) {
        return Hover::Vol(i);
    }
    if let Some(i) = chassis.extras.iter().position(|hit| hit.contains(px, py)) {
        return Hover::Extra(i);
    }
    let d2 = dist2(chassis, px, py);
    if d2 <= CTR_R * CTR_R {
        Hover::Center
    } else if (CTR_R * CTR_R..WHEEL_OUTER * WHEEL_OUTER).contains(&d2) {
        Hover::Sector(ring_sector(chassis, px, py))
    } else {
        Hover::None
    }
}

impl Hit {
    fn contains(&self, px: i32, py: i32) -> bool {
        let dx = px - self.cx;
        let dy = py - self.cy;
        dx * dx + dy * dy <= self.r * self.r
    }
}

fn dist2(chassis: &Chassis, px: i32, py: i32) -> i32 {
    let dx = px - chassis.bezel_cx;
    let dy = py - chassis.bezel_cy;
    dx * dx + dy * dy
}

fn on_wheel_ring(chassis: &Chassis, px: i32, py: i32) -> bool {
    if chassis.vols.iter().any(|hit| hit.contains(px, py)) || chassis.extras.iter().any(|hit| hit.contains(px, py)) {
        return false;
    }
    (CTR_R * CTR_R..WHEEL_OUTER * WHEEL_OUTER).contains(&dist2(chassis, px, py))
}

fn ring_sector(chassis: &Chassis, px: i32, py: i32) -> Input {
    let a = f32::atan2((py - chassis.bezel_cy) as f32, (px - chassis.bezel_cx) as f32);
    let q = PI / 4.0;
    if (-3.0 * q..=-q).contains(&a) {
        Input::North
    } else if (-q..=q).contains(&a) {
        Input::East
    } else if (q..=3.0 * q).contains(&a) {
        Input::South
    } else {
        Input::West
    }
}

fn wheel_angle_at(chassis: &Chassis, px: i32, py: i32) -> f32 {
    f32::atan2((py - chassis.bezel_cy) as f32, (px - chassis.bezel_cx) as f32)
}

fn wrap_delta(from: f32, to: f32) -> f32 {
    let tau = 2.0 * PI;
    (to - from + PI).rem_euclid(tau) - PI
}

fn step_wheel(wheel: &mut Wheel, input: Input) -> Input {
    match input {
        Input::StepLeft => wheel.angle -= WHEEL_DETENT,
        Input::StepRight => wheel.angle += WHEEL_DETENT,
        _ => {}
    }
    input
}

fn compose_window(panel: &[u32], win: &Win, chassis: &Chassis, view: &FrameView, theme: &Theme) -> Vec<u32> {
    let mut out = vec![theme.chassis.background; win.win_w * win.win_h];
    let well = shade(theme.chassis.background, 40);
    fill_rect(
        &mut out,
        win.win_w,
        win.win_h,
        (
            SCREEN_FRAME as i32 - 2,
            SCREEN_FRAME as i32 - 2,
            win.panel_w as i32 + 4,
            win.panel_h as i32 + 4,
        ),
        well,
    );
    for y in 0..win.panel_h {
        let src = y * win.panel_w;
        let dst = (y + SCREEN_FRAME) * win.win_w + SCREEN_FRAME;
        out[dst..dst + win.panel_w].copy_from_slice(&panel[src..src + win.panel_w]);
    }

    let wheel = theme.chassis.button;
    fill_disc(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx,
        chassis.bezel_cy,
        WHEEL_OUTER,
        shade(wheel, 55),
    );
    fill_disc(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx,
        chassis.bezel_cy,
        WHEEL_OUTER - 3,
        shade(wheel, 115),
    );
    fill_disc(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx,
        chassis.bezel_cy,
        WHEEL_OUTER - 6,
        shade(wheel, 100),
    );
    let (sa, ca) = view.wheel_angle.sin_cos();
    let pip_r = (WHEEL_OUTER - 8) as f32;
    plot(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx + (ca * pip_r).round() as i32,
        chassis.bezel_cy + (sa * pip_r).round() as i32,
        shade(theme.chassis.label, 70),
    );

    let center = if matches!(view.hover, Hover::Center) {
        theme.chassis.button_hover
    } else {
        shade(wheel, 80)
    };
    fill_disc(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx,
        chassis.bezel_cy,
        CTR_R + 2,
        shade(wheel, 60),
    );
    fill_disc(&mut out, win.win_w, win.win_h, chassis.bezel_cx, chassis.bezel_cy, CTR_R, center);
    fill_disc(
        &mut out,
        win.win_w,
        win.win_h,
        chassis.bezel_cx,
        chassis.bezel_cy,
        CTR_R - 3,
        shade(center, 90),
    );

    if theme.behavior == Behavior::Mpod {
        let icons = [
            (Input::North, Icon::Menu, 0.0, -1.0),
            (Input::East, Icon::SkipFwd, 1.0, 0.0),
            (Input::South, Icon::PlayPause, 0.0, 1.0),
            (Input::West, Icon::SkipBack, -1.0, 0.0),
        ];
        for (input, icon, dx, dy) in icons {
            let hovered = matches!(view.hover, Hover::Sector(s) if s == input);
            let color = if hovered {
                shade(theme.chassis.label, 160)
            } else {
                theme.chassis.label
            };
            let ix = chassis.bezel_cx + (dx * ICON_R as f32) as i32;
            let iy = chassis.bezel_cy + (dy * ICON_R as f32) as i32;
            draw_icon(&mut out, win.win_w, win.win_h, (ix, iy), icon, color);
        }
    }

    for (i, hit) in chassis.vols.iter().chain(chassis.extras.iter()).enumerate() {
        let hovered = match (i < chassis.vols.len(), view.hover) {
            (true, Hover::Vol(n)) => n == i,
            (false, Hover::Extra(n)) => n == i - chassis.vols.len(),
            _ => false,
        };
        let face = if hovered { theme.chassis.button_hover } else { shade(wheel, 110) };
        fill_disc(&mut out, win.win_w, win.win_h, hit.cx, hit.cy, hit.r, shade(face, 55));
        fill_disc(&mut out, win.win_w, win.win_h, hit.cx, hit.cy - 1, hit.r - 2, face);
        draw_icon(&mut out, win.win_w, win.win_h, (hit.cx, hit.cy), hit.icon, theme.chassis.label);
    }
    out
}

fn shade(color: u32, percent: u32) -> u32 {
    let scale = |c: u32| (c * percent / 100).min(255);
    let r = scale((color >> 16) & 0xff);
    let g = scale((color >> 8) & 0xff);
    let b = scale(color & 0xff);
    (r << 16) | (g << 8) | b
}

fn plot(buf: &mut [u32], stride: usize, win_h: usize, x: i32, y: i32, color: u32) {
    if x >= 0 && y >= 0 && (x as usize) < stride && (y as usize) < win_h {
        buf[y as usize * stride + x as usize] = color;
    }
}

fn fill_disc(buf: &mut [u32], stride: usize, win_h: usize, cx: i32, cy: i32, r: i32, color: u32) {
    if r <= 0 {
        return;
    }
    let r2 = r * r;
    let y0 = (cy - r).max(0);
    let y1 = (cy + r).min(win_h as i32 - 1);
    let x0 = (cx - r).max(0);
    let x1 = (cx + r).min(stride as i32 - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                buf[y as usize * stride + x as usize] = color;
            }
        }
    }
}

fn fill_rect(buf: &mut [u32], stride: usize, win_h: usize, rect: (i32, i32, i32, i32), color: u32) {
    let (x, y, w, h) = rect;
    for dy in 0..h {
        for dx in 0..w {
            plot(buf, stride, win_h, x + dx, y + dy, color);
        }
    }
}

fn fill_triangle(buf: &mut [u32], stride: usize, win_h: usize, pts: [(i32, i32); 3], color: u32) {
    let [(x0, y0), (x1, y1), (x2, y2)] = pts;
    let minx = x0.min(x1).min(x2).max(0);
    let maxx = x0.max(x1).max(x2).min(stride as i32 - 1);
    let miny = y0.min(y1).min(y2).max(0);
    let maxy = y0.max(y1).max(y2).min(win_h as i32 - 1);
    for y in miny..=maxy {
        for x in minx..=maxx {
            let w0 = (x1 - x) * (y2 - y) - (x2 - x) * (y1 - y);
            let w1 = (x2 - x) * (y0 - y) - (x0 - x) * (y2 - y);
            let w2 = (x0 - x) * (y1 - y) - (x1 - x) * (y0 - y);
            if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                buf[y as usize * stride + x as usize] = color;
            }
        }
    }
}

fn draw_icon(buf: &mut [u32], stride: usize, win_h: usize, at: (i32, i32), icon: Icon, color: u32) {
    let (cx, cy) = at;
    match icon {
        Icon::Menu => {
            for i in -1..=1 {
                fill_rect(buf, stride, win_h, (cx - 8, cy + i * 6 - 1, 16, 3), color);
            }
        }
        Icon::Plus => {
            fill_rect(buf, stride, win_h, (cx - 7, cy - 1, 14, 3), color);
            fill_rect(buf, stride, win_h, (cx - 1, cy - 7, 3, 14), color);
        }
        Icon::Minus => fill_rect(buf, stride, win_h, (cx - 7, cy - 1, 14, 3), color),
        Icon::SkipBack => {
            fill_rect(buf, stride, win_h, (cx - 9, cy - 7, 3, 14), color);
            fill_triangle(buf, stride, win_h, [(cx - 5, cy), (cx + 2, cy - 7), (cx + 2, cy + 7)], color);
            fill_triangle(buf, stride, win_h, [(cx + 1, cy), (cx + 8, cy - 7), (cx + 8, cy + 7)], color);
        }
        Icon::SkipFwd => {
            fill_triangle(buf, stride, win_h, [(cx - 8, cy - 7), (cx - 8, cy + 7), (cx - 1, cy)], color);
            fill_triangle(buf, stride, win_h, [(cx - 2, cy - 7), (cx - 2, cy + 7), (cx + 5, cy)], color);
            fill_rect(buf, stride, win_h, (cx + 6, cy - 7, 3, 14), color);
        }
        Icon::PlayPause => {
            fill_triangle(buf, stride, win_h, [(cx - 8, cy - 7), (cx - 8, cy + 7), (cx + 1, cy)], color);
            fill_rect(buf, stride, win_h, (cx + 3, cy - 7, 3, 14), color);
            fill_rect(buf, stride, win_h, (cx + 8, cy - 7, 3, 14), color);
        }
        Icon::Back => {
            fill_triangle(buf, stride, win_h, [(cx - 2, cy), (cx + 8, cy - 7), (cx + 8, cy + 7)], color);
            fill_rect(buf, stride, win_h, (cx - 8, cy - 2, 8, 3), color);
        }
    }
}

fn typed_search_char(window: &Window) -> Option<char> {
    if window.is_key_pressed(Key::Space, KeyRepeat::Yes) {
        return Some(' ');
    }
    const LETTERS: [(Key, char); 26] = [
        (Key::A, 'a'),
        (Key::B, 'b'),
        (Key::C, 'c'),
        (Key::D, 'd'),
        (Key::E, 'e'),
        (Key::F, 'f'),
        (Key::G, 'g'),
        (Key::H, 'h'),
        (Key::I, 'i'),
        (Key::J, 'j'),
        (Key::K, 'k'),
        (Key::L, 'l'),
        (Key::M, 'm'),
        (Key::N, 'n'),
        (Key::O, 'o'),
        (Key::P, 'p'),
        (Key::Q, 'q'),
        (Key::R, 'r'),
        (Key::S, 's'),
        (Key::T, 't'),
        (Key::U, 'u'),
        (Key::V, 'v'),
        (Key::W, 'w'),
        (Key::X, 'x'),
        (Key::Y, 'y'),
        (Key::Z, 'z'),
    ];
    for (key, ch) in LETTERS {
        if window.is_key_pressed(key, KeyRepeat::Yes) {
            return Some(ch);
        }
    }
    const DIGITS: [(Key, char); 10] = [
        (Key::Key0, '0'),
        (Key::Key1, '1'),
        (Key::Key2, '2'),
        (Key::Key3, '3'),
        (Key::Key4, '4'),
        (Key::Key5, '5'),
        (Key::Key6, '6'),
        (Key::Key7, '7'),
        (Key::Key8, '8'),
        (Key::Key9, '9'),
    ];
    for (key, ch) in DIGITS {
        if window.is_key_pressed(key, KeyRepeat::Yes) {
            return Some(ch);
        }
    }
    None
}

#[derive(Default)]
struct MenuHold {
    frames: u16,
    fired: bool,
    /// 0 = none, 1 = M / Menu, 2 = Up / North (mPod letter lists).
    src: u8,
}

const MENU_HOLD_FRAMES: u16 = 15;

fn poll_menu_hold(window: &Window, hold: &mut MenuHold, letter_list: bool, mpod: bool) -> Option<Input> {
    let menu_down = window.is_key_down(Key::M);
    let north_down = mpod && letter_list && window.is_key_down(Key::Up);
    let (down, src) = if menu_down {
        (true, 1u8)
    } else if north_down {
        (true, 2u8)
    } else {
        (false, 0u8)
    };
    if down {
        if hold.frames == 0 {
            hold.src = src;
        }
        hold.frames = hold.frames.saturating_add(1);
        if !hold.fired && hold.frames >= MENU_HOLD_FRAMES {
            hold.fired = true;
            return Some(Input::MenuHold);
        }
        return None;
    }
    if hold.frames > 0 {
        let short = !hold.fired;
        let was = hold.src;
        *hold = MenuHold::default();
        if short {
            return Some(if was == 2 { Input::North } else { Input::Menu });
        }
    }
    None
}

fn poll_input(
    window: &Window,
    chassis: &Chassis,
    wheel: &mut Wheel,
    mouse_was_down: &mut bool,
    menu_hold: &mut MenuHold,
    on_search: bool,
    letter_list: bool,
    mpod: bool,
    show_controls: bool,
) -> Option<Input> {
    if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
        return Some(Input::Quit);
    }
    if window.is_key_pressed(Key::F1, KeyRepeat::No) {
        return Some(Input::Play);
    }
    if window.is_key_pressed(Key::F2, KeyRepeat::No) {
        return Some(Input::Back);
    }
    if on_search {
        if window.is_key_pressed(Key::Backspace, KeyRepeat::Yes) {
            return Some(Input::Backspace);
        }
        if let Some(c) = typed_search_char(window) {
            return Some(Input::Char(c));
        }
    }
    if !on_search {
        if let Some(input) = poll_menu_hold(window, menu_hold, letter_list, mpod) {
            return Some(input);
        }
    }
    let skip_up = mpod && letter_list && (menu_hold.frames > 0 || window.is_key_down(Key::Up));
    if !skip_up && window.is_key_pressed(Key::Up, KeyRepeat::Yes) {
        return Some(Input::North);
    }
    if window.is_key_pressed(Key::Down, KeyRepeat::Yes) {
        return Some(Input::South);
    }
    if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
        return Some(Input::West);
    }
    if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
        return Some(Input::East);
    }
    if window.is_key_pressed(Key::Enter, KeyRepeat::No) || (!on_search && window.is_key_pressed(Key::Space, KeyRepeat::No)) {
        return Some(Input::Center);
    }
    if window.is_key_pressed(Key::LeftBracket, KeyRepeat::Yes) || window.is_key_pressed(Key::Comma, KeyRepeat::Yes) {
        return Some(step_wheel(wheel, Input::StepLeft));
    }
    if window.is_key_pressed(Key::RightBracket, KeyRepeat::Yes) || window.is_key_pressed(Key::Period, KeyRepeat::Yes) {
        return Some(step_wheel(wheel, Input::StepRight));
    }
    if window.is_key_pressed(Key::Minus, KeyRepeat::Yes) {
        return Some(Input::VolDown);
    }
    if window.is_key_pressed(Key::Equal, KeyRepeat::Yes) {
        return Some(Input::VolUp);
    }

    if let Some((sx, sy)) = window.get_scroll_wheel() {
        if sy > 0.0 || (sy == 0.0 && sx < 0.0) {
            return Some(step_wheel(wheel, Input::StepLeft));
        }
        if sy < 0.0 || sx > 0.0 {
            return Some(step_wheel(wheel, Input::StepRight));
        }
    }

    if !show_controls {
        return None;
    }

    let down = window.get_mouse_down(MouseButton::Left);
    let pressed = down && !*mouse_was_down;
    let released = !down && *mouse_was_down;
    *mouse_was_down = down;
    if released {
        let click = match wheel.drag.take() {
            Some(drag) if !drag.rotated => drag.sector,
            _ => None,
        };
        if let Some(input) = click {
            return Some(input);
        }
    }
    if down && let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) {
        let px = x as i32;
        let py = y as i32;
        if wheel.drag.is_some() {
            let a = wheel_angle_at(chassis, px, py);
            let mut step = None;
            if let Some(drag) = wheel.drag.as_mut() {
                drag.accum += wrap_delta(drag.last, a);
                drag.last = a;
                if drag.accum >= WHEEL_DETENT {
                    drag.accum -= WHEEL_DETENT;
                    drag.rotated = true;
                    step = Some(Input::StepRight);
                } else if drag.accum <= -WHEEL_DETENT {
                    drag.accum += WHEEL_DETENT;
                    drag.rotated = true;
                    step = Some(Input::StepLeft);
                }
            }
            return step.map(|input| step_wheel(wheel, input));
        }
        if pressed {
            for hit in chassis.vols.iter().chain(chassis.extras.iter()) {
                if hit.contains(px, py) {
                    return Some(hit.input);
                }
            }
            if dist2(chassis, px, py) <= CTR_R * CTR_R {
                return Some(Input::Center);
            }
            if on_wheel_ring(chassis, px, py) {
                wheel.drag = Some(WheelDrag {
                    last: wheel_angle_at(chassis, px, py),
                    accum: 0.0,
                    sector: Some(ring_sector(chassis, px, py)),
                    rotated: false,
                });
            }
        }
    }
    None
}
