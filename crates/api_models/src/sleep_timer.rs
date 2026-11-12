//! Sleep timer: pause playback after a number of minutes or at the end of
//! the current track.
//!
//! The timer runs on the server (`rsplayer::sleep_timer`), so it works with
//! every client closed. Clients set it with
//! [`crate::common::PlayerCommand::SetSleepTimer`] and learn its state from
//! [`crate::state::StateChangeEvent::SleepTimerEvent`], broadcast on every
//! change and in answer to [`crate::common::PlayerCommand::QuerySleepTimer`].
//! A running countdown is not broadcast every second: clients compute the
//! remaining time from [`SleepTimerState::Until`]'s `ends_at`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Longest accepted timer, in minutes (one day).
pub const SLEEP_TIMER_MAX_MINUTES: u16 = 24 * 60;

/// What the client asks the sleep timer to do. Setting a timer replaces the
/// one that is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SleepTimerRequest {
    /// Pause after this many minutes (1 to [`SLEEP_TIMER_MAX_MINUTES`]).
    Minutes(u16),
    /// Pause when the current track ends (not offered for live radio).
    EndOfTrack,
    /// Cancel the running timer.
    #[default]
    Off,
}

/// The sleep timer as the server holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SleepTimerState {
    #[default]
    Off,
    /// Playback pauses at `ends_at`.
    Until { ends_at: DateTime<Utc> },
    /// Playback pauses when the current track ends.
    EndOfTrack,
}

impl SleepTimerState {
    /// A timer that ends `minutes` from now.
    #[must_use]
    pub fn minutes_from_now(minutes: u16) -> Self {
        Self::Until {
            ends_at: Utc::now() + chrono::Duration::minutes(i64::from(minutes)),
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::Off)
    }
}
