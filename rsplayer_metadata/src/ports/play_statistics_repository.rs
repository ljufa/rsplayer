use std::sync::Arc;

use api_models::stat::PlayItemStatistics;

use crate::error::RepoResult;

pub trait PlayStatisticsRepository: Send + Sync {
    fn find_by_id(&self, play_item_id: &str) -> Option<PlayItemStatistics>;
    fn find_by_key_prefix(&self, prefix: &str) -> Vec<PlayItemStatistics>;
    fn get_all(&self) -> Vec<PlayItemStatistics>;
    fn save(&self, play_item_statistics: &PlayItemStatistics) -> RepoResult<()>;
}

pub type ArcPlayStatisticsRepository = Arc<dyn PlayStatisticsRepository>;
