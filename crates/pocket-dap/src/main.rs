use std::path::PathBuf;

use rsplayer_pocket_dap::{DEFAULT_PROFILE, DisplayKind, Theme, WindowConfig, run_window};

fn main() {
    let cfg = Config::from_args(std::env::args().skip(1));
    if cfg.help {
        print_help();
        return;
    }
    if cfg.backend != Backend::Window {
        eprintln!("SPI backend is not implemented yet; use --backend window (default)");
        std::process::exit(1);
    }
    let name = cfg.profile.as_deref().unwrap_or(DEFAULT_PROFILE);
    let theme = match Theme::load(name, cfg.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let display = cfg.display.unwrap_or(DisplayKind::St7789);
    let fit_screen = cfg.display.is_none() || cfg.full_screen;
    run_window(WindowConfig {
        ws_url: cfg.ws,
        display,
        show_controls: cfg.show_controls,
        full_screen: cfg.full_screen,
        fit_screen,
        theme,
    })
    .expect("window");
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Backend {
    Window,
    Spi,
}

struct Config {
    /// `None` = default ST7789 and FitScreen scaling.
    display: Option<DisplayKind>,
    backend: Backend,
    show_controls: bool,
    full_screen: bool,
    ws: String,
    theme: Option<PathBuf>,
    profile: Option<String>,
    help: bool,
}

impl Config {
    fn from_args(args: impl Iterator<Item = String>) -> Self {
        let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
        let mut cfg = Self {
            display: None,
            backend: Backend::Window,
            show_controls: false,
            full_screen: false,
            ws: format!("ws://127.0.0.1:{port}/api/ws"),
            theme: None,
            profile: None,
            help: false,
        };
        let args: Vec<String> = args.collect();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => cfg.help = true,
                "--show-controls" => cfg.show_controls = true,
                "--full-screen" => cfg.full_screen = true,
                "--display" => {
                    i += 1;
                    cfg.display = match args.get(i).map(String::as_str) {
                        Some("st7789") => Some(DisplayKind::St7789),
                        Some("st7567") => Some(DisplayKind::St7567),
                        other => {
                            eprintln!("unknown --display {:?}", other.unwrap_or(""));
                            cfg.help = true;
                            None
                        }
                    };
                }
                "--backend" => {
                    i += 1;
                    cfg.backend = match args.get(i).map(String::as_str) {
                        Some("window") => Backend::Window,
                        Some("spi") => Backend::Spi,
                        other => {
                            eprintln!("unknown --backend {:?}", other.unwrap_or(""));
                            cfg.help = true;
                            Backend::Window
                        }
                    };
                }
                "--ws" => {
                    i += 1;
                    if let Some(url) = args.get(i) {
                        cfg.ws = url.clone();
                    }
                }
                "--profile" => {
                    i += 1;
                    if let Some(name) = args.get(i) {
                        cfg.profile = Some(name.clone());
                    }
                }
                "--theme" => {
                    i += 1;
                    if let Some(path) = args.get(i) {
                        cfg.theme = Some(PathBuf::from(path));
                    }
                }
                flag => {
                    eprintln!("unknown argument {flag}");
                    cfg.help = true;
                }
            }
            i += 1;
        }
        cfg
    }
}

fn print_help() {
    eprintln!(
        "\
rsplayer-pocket-dap — DAP front panel for a running rsplayer

  --backend window|spi       window (default): X11/HDMI UI
                             spi: hardware panel only, no window (not implemented)
  --show-controls             draw chassis chrome (click wheel / side buttons) for
                             mouse or touch; omit for panel pixels only
  --full-screen              borderless window scaled to the screen (FitScreen)
  --display st7789|st7567    logical panel framebuffer; omit = st7789 and FitScreen
  --ws ws://HOST:PORT/api/ws default ws://127.0.0.1:$PORT/api/ws (PORT or 8000)
  --profile NAME             creative (default) | mpod | pocket-dap | playlist
                             mpod        Menu=back, South=play/pause, left/right=skip,
                                         wheel=volume on Now Playing
                             creative    side Play/Pause and Back; wheel scrolls lists
                             pocket-dap  South=play/pause; wheel=volume on Now Playing
                             playlist    Win Amp — South=play/pause; wheel scrolls; playlist colors
  --theme path.json          optional color file for that profile (default: profiles/NAME.json)
                             colors, optional \"font\" (7x13 or 9x15), optional seven_segment
                             (no separate CLI flags for those). Behavior and layout stay in code.
                             Color skins for pocket-dap: profiles/classic-green.json,
                             profiles/lcd-ice.json

To add a player: copy profiles/creative.json, then add one row in src/theme.rs (`KNOWN`).

Window UI (keyboard always; chassis only with --show-controls):
  mPod wheel   MENU / skip / play-pause icons only for --profile mpod
  other wheels same wheel, no Menu / Play / Pause icons
  creative     two side buttons: Play/Pause (F1) and Back (F2)
  encoder      drag the ring, mouse wheel, or [ ]
  GPIO         + -                                 = -   volume on every profile
  Menu GPIO    M (short = main menu; hold on Artists/Albums/Playlists/Tracks = letter jump)
  quit         Esc
  search       Library > Search: type letters; encoder picks [a]; Center appends; Right = results
  letters      hold Menu (mPod: hold Up) or spin the wheel fast on long library lists

SPI mode has no X window (not implemented yet)."
    );
}
