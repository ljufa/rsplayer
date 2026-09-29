//! ANO five-way plus encoder step.
//!
//! What `South` and the wheel do depends on [`crate::theme::Behavior`].
//! GPIO `VolDown` / `VolUp` change volume on every screen and every profile.
//! Menu GPIO is optional. Hardware pins are not decided.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    North,
    South,
    West,
    East,
    Center,
    /// One detent. Meaning depends on the active profile.
    StepLeft,
    StepRight,
    /// GPIO volume buttons (emulated in the chassis until wired).
    VolDown,
    VolUp,
    /// Optional GPIO Menu (emulated). On mPod the wheel's Menu sector is North, not this key.
    Menu,
    /// Menu (or mPod North) held ~0.5 s on a long library list: open letter picker.
    MenuHold,
    /// Creative Zen side button: play/pause on every screen.
    Play,
    /// Creative Zen side button: leave the current screen.
    Back,
    /// Search query typing (window keyboard; encoder picks letters on Search).
    Char(char),
    Backspace,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayKind {
    St7789,
    St7567,
}
