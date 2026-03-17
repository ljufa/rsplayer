use api_models::stat::PlayItemStatistics;
use fjall::{Database, Keyspace, KeyspaceCreateOptions};

pub struct PlayStatisticsRepository {
    pub db: Keyspace,
}

impl PlayStatisticsRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            db: db
                .keyspace("play_statistics", KeyspaceCreateOptions::default)
                .expect("Failed to open play_statistics keyspace"),
        }
    }

    pub fn find_by_id(&self, play_item_id: &str) -> Option<PlayItemStatistics> {
        let data = self.db.get(play_item_id).expect("Failed to get play item statistics")?;
        let json = String::from_utf8(data.to_vec()).unwrap();
        Some(serde_json::from_str(&json).unwrap())
    }
    pub fn find_by_key_prefix(&self, prefix: &str) -> Vec<PlayItemStatistics> {
        self.db
            .prefix(prefix)
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                let json = String::from_utf8(value.to_vec()).ok()?;
                serde_json::from_str(&json).ok()
            })
            .collect()
    }

    pub fn get_all(&self) -> Vec<PlayItemStatistics> {
        self.db
            .iter()
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                let json = String::from_utf8(value.to_vec()).ok()?;
                serde_json::from_str::<PlayItemStatistics>(&json).ok()
            })
            .collect()
    }

    pub fn save(&self, play_item_statistics: &PlayItemStatistics) {
        let play_item_id = play_item_statistics.play_item_id.clone();
        let json = serde_json::to_string(play_item_statistics).unwrap();
        self.db
            .insert(play_item_id, json.as_bytes())
            .expect("Failed to save play item statistics");
    }
}
impl PlayStatisticsRepository {
    pub fn new_standalone(db_path: &str) -> Self {
        let db = Database::builder(db_path).open().expect("Failed to open statistics db");
        Self {
            db: db
                .keyspace("play_statistics", KeyspaceCreateOptions::default)
                .expect("Failed to open play_statistics keyspace"),
        }
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
        let play_statistics_repository = super::PlayStatisticsRepository::new_standalone("/tmp/test_stats_fjall.db");
        play_statistics_repository.save(&play_item_statistics);
        let play_item_statistics = play_statistics_repository.find_by_id("test");
        assert!(play_item_statistics.is_some());
        let play_item_statistics = play_item_statistics.unwrap();
        assert_eq!(play_item_statistics.play_count, 1);
        assert_eq!(play_item_statistics.play_item_id, "test");
    }
}
