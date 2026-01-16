use api_models::stat::PlayItemStatistics;
use sled::Db;

pub struct PlayStatisticsRepository {
    pub db: Db,
}

impl PlayStatisticsRepository {
    pub fn new(db_path: &str) -> Self {
        let db = sled::open(db_path).expect("Failed to open statistics db");
        Self { db }
    }

    pub fn find_by_id(&self, play_item_id: &str) -> Option<PlayItemStatistics> {
        let play_item_statistics_json = self.db.get(play_item_id).expect("Failed to get play item statistics");
        play_item_statistics_json.map(|play_item_statistics_json| {
            let play_item_statistics_json = play_item_statistics_json.to_vec();
            let play_item_statistics_json = String::from_utf8(play_item_statistics_json).unwrap();
            let play_item_statistics: PlayItemStatistics = serde_json::from_str(&play_item_statistics_json).unwrap();
            play_item_statistics
        })
    }
    pub fn find_by_key_prefix(&self, prefix: &str) -> Vec<PlayItemStatistics> {
        let mut play_item_statistics = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item.expect("Failed to get play item statistics");
            let play_item_statistics_json = value.to_vec();
            let play_item_statistics_json = String::from_utf8(play_item_statistics_json).unwrap();
            let stat: PlayItemStatistics = serde_json::from_str(&play_item_statistics_json).unwrap();
            play_item_statistics.push(stat);
        }
        play_item_statistics
    }

    pub fn get_all(&self) -> Vec<PlayItemStatistics> {
        self.db
            .iter()
            .filter_map(Result::ok)
            .map(|(_, value)| {
                let json = String::from_utf8(value.to_vec()).unwrap_or_default();
                serde_json::from_str::<PlayItemStatistics>(&json).ok()
            })
            .flatten()
            .collect()
    }

    pub fn save(&self, play_item_statistics: &PlayItemStatistics) {
        let play_item_id = play_item_statistics.play_item_id.clone();
        let play_item_statistics_json = serde_json::to_string(play_item_statistics).unwrap();
        self.db
            .insert(play_item_id, play_item_statistics_json.as_bytes())
            .expect("Failed to save play item statistics");
        self.db.flush().expect("Failed to flush play item statistics");
    }
}
impl Default for PlayStatisticsRepository {
    fn default() -> Self {
        Self::new("play_statistics.db")
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {
        let play_item_statistics = super::PlayItemStatistics {
            play_item_id: "test".to_string(),
            play_count: 1,
            last_played: Some(chrono::Local::now()),
            skipped_count: 0,
            liked_count: 0,
        };
        let play_statistics_repository = super::PlayStatisticsRepository::new("/tmp/test.db");
        play_statistics_repository.save(&play_item_statistics);
        let play_item_statistics = play_statistics_repository.find_by_id("test");
        assert!(play_item_statistics.is_some());
        let play_item_statistics = play_item_statistics.unwrap();
        assert_eq!(play_item_statistics.play_count, 1);
        assert_eq!(play_item_statistics.play_item_id, "test");
    }
}
