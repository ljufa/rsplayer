//! One JSON file per behavior profile under `profiles/`.
//!
//! The file is colors, optional font, and optional `seven_segment` only — no
//! CLI flags for those. Behavior and layout are the rows in `KNOWN`.
//! Extra color skins for a profile (for example `classic-green.json` and
//! `lcd-ice.json` for `pocket-dap`) are the same JSON shape and load with
//! `--theme path.json`; they are not `--profile` names.
//! To add a player: drop `profiles/<name>.json`, then add one row. A new
//! click-wheel mapping also needs a `Behavior` arm in `app`.

use std::fs;
use std::path::Path;

use embedded_graphics::pixelcolor::Rgb565;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub behavior: Behavior,
    pub layout: Layout,
    pub font: Face,
    pub seven_segment: bool,
    pub color: ColorTheme,
    pub chassis: ChassisTheme,
}

/// How the click wheel and Play/Pause map. Sony and iRiver are not implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    /// House controls: Play/Pause on every screen. Wheel is volume on Now Playing and list scroll elsewhere.
    PocketDap,
    /// Click-wheel mPod: Menu goes back, Play/Pause is the bottom of the wheel on every screen,
    /// left/right skip tracks, the wheel is volume on Now Playing and list scroll elsewhere.
    Mpod,
    /// Zen V: side Play and Back. The wheel scrolls lists and does not carry mPod icons.
    Creative,
    /// Win Amp playlist colors. Play/Pause stays on South. The wheel only scrolls lists.
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Lcd,
    Mpod,
    Playlist,
}

/// Built-in mono face named by the profile JSON. `7x13` is the list size used today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Mono7,
    Mono9,
}

impl Face {
    pub fn px(self) -> i32 {
        match self {
            Face::Mono7 => 7,
            Face::Mono9 => 9,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColorTheme {
    pub background: Rgb565,
    pub foreground: Rgb565,
    pub dim: Rgb565,
    pub accent: Rgb565,
    pub highlight: Rgb565,
    pub highlight_fg: Rgb565,
    pub header: Rgb565,
    pub bar: Rgb565,
    pub track: Rgb565,
}

#[derive(Debug, Clone, Copy)]
pub struct ChassisTheme {
    pub background: u32,
    pub button: u32,
    pub button_hover: u32,
    pub label: u32,
}

struct Known {
    name: &'static str,
    json: &'static str,
    behavior: Behavior,
    layout: Layout,
}

/// Colors live in the JSON. This table is the behavior.
const KNOWN: &[Known] = &[
    Known {
        name: "mpod",
        json: include_str!("../profiles/mpod.json"),
        behavior: Behavior::Mpod,
        layout: Layout::Mpod,
    },
    Known {
        name: "creative",
        json: include_str!("../profiles/creative.json"),
        behavior: Behavior::Creative,
        layout: Layout::Lcd,
    },
    Known {
        name: "pocket-dap",
        json: include_str!("../profiles/pocket-dap.json"),
        behavior: Behavior::PocketDap,
        layout: Layout::Mpod,
    },
    Known {
        name: "playlist",
        json: include_str!("../profiles/playlist.json"),
        behavior: Behavior::Playlist,
        layout: Layout::Playlist,
    },
];

pub const DEFAULT_PROFILE: &str = "creative";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkinFile {
    #[serde(default)]
    seven_segment: bool,
    #[serde(default)]
    font: FontName,
    color: ColorFile,
    chassis: ChassisFile,
}

#[derive(Deserialize, Default, Clone, Copy)]
enum FontName {
    #[default]
    #[serde(rename = "7x13")]
    Mono7,
    #[serde(rename = "9x15")]
    Mono9,
}

#[derive(Deserialize)]
struct ColorFile {
    background: String,
    foreground: String,
    dim: String,
    accent: String,
    highlight: String,
    highlight_fg: String,
    header: String,
    bar: String,
    track: String,
}

#[derive(Deserialize)]
struct ChassisFile {
    background: String,
    button: String,
    button_hover: String,
    label: String,
}

impl Theme {
    /// `colors` replaces the bundled palette for `profile`. It does not change behavior.
    pub fn load(profile: &str, colors: Option<&Path>) -> Result<Self, String> {
        let Some(known) = KNOWN.iter().find(|k| k.name == profile) else {
            let names: Vec<&str> = KNOWN.iter().map(|k| k.name).collect();
            return Err(format!("unknown profile {profile:?}; known: {names:?}"));
        };
        let owned;
        let text = match colors {
            None => known.json,
            Some(p) => {
                owned = fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
                owned.as_str()
            }
        };
        let skin: SkinFile = serde_json::from_str(text).map_err(|e| format!("profile JSON: {e}"))?;
        Ok(Self {
            name: profile.to_string(),
            behavior: known.behavior,
            layout: known.layout,
            font: match skin.font {
                FontName::Mono7 => Face::Mono7,
                FontName::Mono9 => Face::Mono9,
            },
            seven_segment: skin.seven_segment,
            color: ColorTheme {
                background: rgb565(&skin.color.background)?,
                foreground: rgb565(&skin.color.foreground)?,
                dim: rgb565(&skin.color.dim)?,
                accent: rgb565(&skin.color.accent)?,
                highlight: rgb565(&skin.color.highlight)?,
                highlight_fg: rgb565(&skin.color.highlight_fg)?,
                header: rgb565(&skin.color.header)?,
                bar: rgb565(&skin.color.bar)?,
                track: rgb565(&skin.color.track)?,
            },
            chassis: ChassisTheme {
                background: argb(&skin.chassis.background)?,
                button: argb(&skin.chassis.button)?,
                button_hover: argb(&skin.chassis.button_hover)?,
                label: argb(&skin.chassis.label)?,
            },
        })
    }
}

fn parse_hex(hex: &str) -> Result<(u8, u8, u8), String> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return Err(format!("expected #RRGGBB, got {hex}"));
    }
    let n = u32::from_str_radix(h, 16).map_err(|_| format!("invalid hex {hex}"))?;
    Ok((((n >> 16) & 0xff) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8))
}

fn rgb565(hex: &str) -> Result<Rgb565, String> {
    let (r, g, b) = parse_hex(hex)?;
    Ok(Rgb565::new(r >> 3, g >> 2, b >> 3))
}

fn argb(hex: &str) -> Result<u32, String> {
    let (r, g, b) = parse_hex(hex)?;
    Ok((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_profiles_load() {
        let mpod = Theme::load("mpod", None).expect("mpod");
        assert_eq!(mpod.behavior, Behavior::Mpod);
        assert_eq!(mpod.layout, Layout::Mpod);
        assert_eq!(mpod.font, Face::Mono7);
        let creative = Theme::load("creative", None).expect("creative");
        assert_eq!(creative.behavior, Behavior::Creative);
        assert!(Theme::load("nope", None).is_err());
        assert!(Theme::load("classic-green", None).is_err());
        assert!(Theme::load("lcd-ice", None).is_err());
    }

    #[test]
    fn pocket_dap_accepts_color_skins() {
        let green = Theme::load(
            "pocket-dap",
            Some(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/profiles/classic-green.json"))),
        )
        .expect("classic-green skin");
        assert_eq!(green.behavior, Behavior::PocketDap);
        assert_eq!(green.layout, Layout::Mpod);
        let ice = Theme::load(
            "pocket-dap",
            Some(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/profiles/lcd-ice.json"))),
        )
        .expect("lcd-ice skin");
        assert_eq!(ice.behavior, Behavior::PocketDap);
        assert!(!ice.seven_segment);
    }
}
