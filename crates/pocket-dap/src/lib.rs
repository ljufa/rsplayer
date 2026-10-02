//! Dedicated pocket DAP UI for RSPlayer.
//!
//! The same framebuffer can drive a real SPI panel later. The default path is
//! `--backend window`: an X11/HDMI window with the panel pixels, and optional
//! chassis chrome via `--show-controls` (click wheel, GPIO volume; ring / mouse
//! wheel / `[` `]`). The wheel's meaning depends on the profile (`--profile
//! mpod`, `creative`, `pocket-dap`, `playlist`). Playback control goes to a
//! running `rsplayer` over `/api/ws`. SPI mode has no X window.

mod app;
mod draw_bw;
mod draw_color;
mod fb;
mod host;
mod input;
mod lcd;
mod text;
mod theme;
mod window;

pub use input::DisplayKind;
pub use theme::{DEFAULT_PROFILE, Theme};
pub use window::{WindowConfig, run_window};
