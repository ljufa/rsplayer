//! Podcast subscriptions and episodes.
//!
//! A [`Podcast`] is a subscribed feed; its [`Episode`]s are cached from the
//! feed and carry per-listener state (`position_secs`, `played`). An episode
//! enters the playback queue as an ordinary [`Song`] whose `file` is the
//! enclosure URL (see [`Episode::to_song`]) — playback itself has no podcast
//! knowledge; the `podcast_episode_id` tag lets the podcast service recognise
//! its episodes in `CurrentSongEvent` and resume them.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::player::Song;

pub const EPISODE_ID_TAG: &str = "podcast_episode_id";
pub const PODCAST_ID_TAG: &str = "podcast_id";

/// A subscribed feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Podcast {
    /// Stable id derived from the feed URL.
    pub id: String,
    pub feed_url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub subscribed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_refreshed: Option<DateTime<Utc>>,
    /// Error message of the last failed refresh, cleared on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// HTTP validators for conditional refresh (`If-None-Match` / `If-Modified-Since`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub episode_count: u32,
    /// Episodes not yet played.
    #[serde(default)]
    pub unplayed_count: u32,
}

/// One item of a feed plus the listener's progress through it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Episode {
    /// Stable id derived from the podcast id and the feed item guid.
    pub id: String,
    pub podcast_id: String,
    pub guid: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub published: Option<DateTime<Utc>>,
    /// Enclosure URL — also the queue/`Song::file` key while playing.
    pub audio_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Last known playback position; 0 when never started or finished.
    #[serde(default)]
    pub position_secs: u64,
    #[serde(default)]
    pub played: bool,
    /// Reserved for offline downloads (not populated yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

impl Episode {
    /// The queue representation of this episode. `podcast` supplies the
    /// show-level fields (artist/album/artwork fallback).
    #[must_use]
    pub fn to_song(&self, podcast: &Podcast) -> Song {
        let mut tags = HashMap::new();
        tags.insert(EPISODE_ID_TAG.to_string(), self.id.clone());
        tags.insert(PODCAST_ID_TAG.to_string(), self.podcast_id.clone());
        Song {
            file: self.audio_url.clone(),
            title: Some(self.title.clone()),
            artist: Some(podcast.title.clone()),
            album: Some(podcast.title.clone()),
            album_artist: podcast.author.clone(),
            genre: Some("Podcast".to_string()),
            date: self.published.map(|d| d.format("%Y-%m-%d").to_string()),
            time: self.duration_secs.map(Duration::from_secs),
            image_url: self.image_url.clone().or_else(|| podcast.image_url.clone()),
            tags,
            ..Default::default()
        }
    }

    /// Fraction listened, 0.0..=1.0, when the duration is known.
    #[must_use]
    pub fn progress_fraction(&self) -> Option<f64> {
        let total = self.duration_secs.filter(|d| *d > 0)?;
        #[allow(clippy::cast_precision_loss)]
        Some((self.position_secs as f64 / total as f64).clamp(0.0, 1.0))
    }
}

impl Song {
    /// Episode id when this song was queued from a podcast.
    #[must_use]
    pub fn podcast_episode_id(&self) -> Option<&str> {
        self.tags.get(EPISODE_ID_TAG).map(String::as_str)
    }

    #[must_use]
    pub fn podcast_id(&self) -> Option<&str> {
        self.tags.get(PODCAST_ID_TAG).map(String::as_str)
    }
}

/// Which directory answers searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PodcastDirectory {
    #[default]
    Itunes,
    PodcastIndex,
}

/// A directory search hit; `feed_url` is what `Subscribe` needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PodcastSearchResult {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub feed_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_count: Option<u32>,
    pub source: PodcastDirectory,
}

/// One page of a podcast's episodes, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodePage {
    pub podcast_id: String,
    pub total: usize,
    pub offset: usize,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PodcastCommand {
    /// Search the configured directory. Answered by `PodcastSearchResults`.
    Search(String),
    /// Fetch the feed, store it and its episodes. Answered by `Podcasts`.
    Subscribe { feed_url: String },
    Unsubscribe(String),
    /// Answered by `Podcasts`.
    QueryPodcasts,
    /// Answered by `PodcastEpisodes`.
    QueryEpisodes { podcast_id: String, offset: usize, limit: usize },
    /// Re-fetch one feed, or all when `None`.
    Refresh(Option<String>),
    PlayEpisode(String),
    AddEpisodeToQueue(String),
    AddEpisodeAfterCurrent(String),
    /// Mark played (`true`, also clears the position) or unplayed.
    SetPlayed(String, bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_song_carries_episode_identity_and_show_fallbacks() {
        let podcast = Podcast {
            id: "p1".into(),
            title: "Show".into(),
            author: Some("Host".into()),
            image_url: Some("https://x/show.jpg".into()),
            ..Default::default()
        };
        let ep = Episode {
            id: "e1".into(),
            podcast_id: "p1".into(),
            title: "Ep".into(),
            audio_url: "https://x/ep.mp3".into(),
            duration_secs: Some(90),
            ..Default::default()
        };
        let song = ep.to_song(&podcast);
        assert_eq!(song.file, "https://x/ep.mp3");
        assert_eq!(song.podcast_episode_id(), Some("e1"));
        assert_eq!(song.podcast_id(), Some("p1"));
        assert_eq!(song.artist.as_deref(), Some("Show"));
        assert_eq!(song.image_url.as_deref(), Some("https://x/show.jpg"));
        assert_eq!(song.time, Some(Duration::from_secs(90)));
    }

    #[test]
    fn progress_fraction_clamps_and_handles_unknown_duration() {
        let mut ep = Episode {
            position_secs: 30,
            duration_secs: Some(60),
            ..Default::default()
        };
        assert_eq!(ep.progress_fraction(), Some(0.5));
        ep.position_secs = 100;
        assert_eq!(ep.progress_fraction(), Some(1.0));
        ep.duration_secs = None;
        assert_eq!(ep.progress_fraction(), None);
    }
}
