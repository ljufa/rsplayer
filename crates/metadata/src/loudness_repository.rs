use fjall::{Database, Keyspace, KeyspaceCreateOptions};

use crate::error::{RepoError, RepoResult};
pub use crate::ports::loudness_repository::{ArcLoudnessRepository, LoudnessRepository};

pub struct FjallLoudnessRepository {
    db: Keyspace,
}

impl FjallLoudnessRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            db: db
                .keyspace("loudness", KeyspaceCreateOptions::default)
                .expect("Failed to open loudness keyspace"),
        }
    }

    /// Standalone constructor for tests — opens its own fjall Database.
    pub fn new_standalone(db_path: &str) -> Self {
        let db = Database::builder(db_path).open().expect("Failed to open loudness db");
        Self {
            db: db
                .keyspace("loudness", KeyspaceCreateOptions::default)
                .expect("Failed to open loudness keyspace"),
        }
    }
}

impl LoudnessRepository for FjallLoudnessRepository {
    fn get(&self, file_key: &str) -> Option<i32> {
        let bytes = self.db.get(file_key).ok()??;
        match bytes.as_ref() {
            [0x01, a, b, c, d] => Some(i32::from_le_bytes([*a, *b, *c, *d])),
            _ => None,
        }
    }

    fn contains(&self, file_key: &str) -> bool {
        self.db.contains_key(file_key).unwrap_or(false)
    }

    fn save_loudness(&self, file_key: &str, loudness: i32) -> RepoResult<()> {
        let mut bytes = [0u8; 5];
        bytes[0] = 0x01;
        bytes[1..5].copy_from_slice(&loudness.to_le_bytes());
        self.db
            .insert(file_key, bytes.as_ref())
            .map_err(|e| RepoError::Storage(format!("save loudness for '{file_key}': {e}")))
    }

    fn save_unavailable(&self, file_key: &str) -> RepoResult<()> {
        self.db
            .insert(file_key, &[0x00u8][..])
            .map_err(|e| RepoError::Storage(format!("save loudness sentinel for '{file_key}': {e}")))
    }

    fn count_analysed(&self) -> usize {
        self.db.approximate_len()
    }

    fn flush(&self) {
        // fjall handles persistence at the Database level; no-op here.
    }

    fn delete_all(&self) {
        let keys: Vec<Vec<u8>> = self
            .db
            .iter()
            .filter_map(|guard| guard.key().ok().map(|k| k.to_vec()))
            .collect();
        for key in keys {
            _ = self.db.remove(key);
        }
    }
}
