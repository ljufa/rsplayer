use sled::Db;

pub struct LoudnessRepository {
    db: Db,
}

impl LoudnessRepository {
    pub fn new(db_path: &str) -> Self {
        Self {
            db: sled::open(db_path).expect("Failed to open loudness db"),
        }
    }

    /// Returns the stored loudness in hundredths of a LUFS, or `None` if the
    /// file has not been analysed yet.  Files analysed but with no measurable
    /// loudness (DSD, decode errors) are also stored and return `None`.
    pub fn get(&self, file_key: &str) -> Option<i32> {
        let bytes = self.db.get(file_key).ok()??;
        match bytes.as_ref() {
            [0x01, a, b, c, d] => Some(i32::from_le_bytes([*a, *b, *c, *d])),
            _ => None,
        }
    }

    /// Returns true if the file has already been analysed (regardless of
    /// whether a valid loudness value was obtained).
    pub fn contains(&self, file_key: &str) -> bool {
        self.db.contains_key(file_key).unwrap_or(false)
    }

    pub fn save_loudness(&self, file_key: &str, loudness: i32) {
        let mut bytes = [0u8; 5];
        bytes[0] = 0x01;
        bytes[1..5].copy_from_slice(&loudness.to_le_bytes());
        self.db.insert(file_key, bytes.as_ref()).expect("Failed to save loudness");
    }

    /// Mark the file as analysed but without a usable loudness value.
    pub fn save_unavailable(&self, file_key: &str) {
        self.db.insert(file_key, &[0x00u8][..]).expect("Failed to save loudness sentinel");
    }

    /// Total number of songs that have been processed (includes unavailable sentinels).
    pub fn count_analysed(&self) -> usize {
        self.db.len()
    }

    pub fn flush(&self) {
        let _ = self.db.flush();
    }

    pub fn delete_all(&self) {
        self.db.clear().expect("Failed to clear loudness db");
        let _ = self.db.flush();
    }
}

impl Default for LoudnessRepository {
    fn default() -> Self {
        Self::new("loudness.db")
    }
}
