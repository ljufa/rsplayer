use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct PlayItemStatistics {
    pub play_item_id: String,
    pub play_count: i32,
    pub last_played: Option<DateTime<Local>>,
    pub skipped_count: i32,
    pub liked_count: i32,
}
