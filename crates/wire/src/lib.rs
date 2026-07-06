//! Protocol shared between the server and the `RSPlayer` front-panel firmware
//! (USB serial; see the separate `rsplayer_firmware` repository).
//!
//! `no_std` because the firmware side compiles it for a microcontroller:
//! strings are fixed-capacity `heapless` types and messages are
//! postcard-encoded, COBS-framed (bounded by [`MAX_FRAME`]). [`HostToFw`]
//! pushes display/volume state to the panel; [`FwToHost`] carries knob,
//! button and power events back. Unrelated to the multiroom sync protocol in
//! `crates/sync`.

#![no_std]

pub use heapless;
pub use postcard;

use heapless::String;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, IntoStaticStr};

pub const TITLE_LEN: usize = 64;
pub const ARTIST_LEN: usize = 64;
pub const ALBUM_LEN: usize = 64;
pub const TIME_LEN: usize = 16;

/// Upper bound on a single COBS-framed wire message. Sized to comfortably hold
/// the largest variant (Track with three 64-byte strings + length prefixes +
/// COBS overhead) on both encode and decode buffers.
pub const MAX_FRAME: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, EnumString, EnumIter, IntoStaticStr)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PlaybackMode {
    #[default]
    Sequential,
    Random,
    LoopSingle,
    LoopQueue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostToFw {
    SetVolume(u8),
    VolumeUp,
    VolumeDown,
    QueryVolume,
    PowerOn,
    PowerOff,
    Track {
        title: String<TITLE_LEN>,
        artist: String<ARTIST_LEN>,
        album: String<ALBUM_LEN>,
    },
    Progress {
        current: String<TIME_LEN>,
        total: String<TIME_LEN>,
        percent: u8,
    },
    Vu {
        left: u8,
        right: u8,
    },
    PlaybackMode(PlaybackMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FwPlayerCmd {
    Next,
    Prev,
    TogglePlay,
    Stop,
    SeekForward,
    SeekBackward,
    CyclePlaybackMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FwToHost {
    Volume(u8),
    Power(bool),
    Player(FwPlayerCmd),
}
