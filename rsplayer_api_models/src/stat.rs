use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct LibraryStats {
    pub total_songs: usize,
    pub total_albums: usize,
    pub total_artists: usize,
    /// Sum of all song durations in seconds.
    pub total_duration_secs: u64,
    pub total_plays: u32,
    /// Songs that have been played at least once.
    pub unique_songs_played: usize,
    pub liked_songs: usize,
    /// Songs that have been analysed for loudness.
    pub songs_loudness_analysed: usize,
    /// (genre_name, song_count), sorted by count descending.
    pub top_genres: Vec<(String, usize)>,
    /// (decade_label, album_count), sorted chronologically.
    pub albums_by_decade: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct PlayItemStatistics {
    pub play_item_id: String,
    pub play_count: i32,
    pub last_played: Option<DateTime<Local>>,
    pub skipped_count: i32,
    pub liked_count: i32,
}
