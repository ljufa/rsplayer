//! Podcast subscriptions: directory search, feed refresh, episode cache and
//! per-episode resume.
//!
//! Playback needs no podcast knowledge — an episode is queued as a `Song`
//! whose `file` is the enclosure URL (`Episode::to_song`). This crate owns
//! everything around that: [`directory`] finds feeds, [`feed`] parses them,
//! [`repository`] stores shows/episodes in fjall, and [`service`] runs the
//! worker thread (search/subscribe/refresh) plus the progress tracker that
//! turns `SongTimeEvent`s into resume positions and `played` flags.

pub mod directory;
pub mod feed;
pub mod repository;
pub mod service;

pub use service::{PodcastJob, PodcastService};
