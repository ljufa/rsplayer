//! What the player is playing from.
//!
//! The queue is only one source: a radio station or a podcast episode is
//! played directly, without being added to the queue, so the queue (and its
//! position) survives a detour to the radio. Next/Prev and end-of-track
//! behaviour depend on the source: the queue advances, radio cycles through
//! the favorite and custom stations, podcasts continue with the next
//! unplayed episode of the same show.

use serde::{Deserialize, Serialize};

use crate::radio::RadioStation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlaybackSource {
    #[default]
    Queue,
    Radio(RadioStation),
    Podcast { podcast_id: String, episode_id: String },
}

impl PlaybackSource {
    #[must_use]
    pub const fn is_queue(&self) -> bool {
        matches!(self, Self::Queue)
    }

    #[must_use]
    pub const fn is_radio(&self) -> bool {
        matches!(self, Self::Radio(_))
    }

    #[must_use]
    pub const fn is_podcast(&self) -> bool {
        matches!(self, Self::Podcast { .. })
    }
}
