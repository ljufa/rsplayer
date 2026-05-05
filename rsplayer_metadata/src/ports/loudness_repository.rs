use std::sync::Arc;

use crate::error::RepoResult;

pub trait LoudnessRepository: Send + Sync {
    /// Returns the stored loudness in hundredths of a LUFS, or `None` if the
    /// file has not been analysed yet (or analysed but unmeasurable).
    fn get(&self, file_key: &str) -> Option<i32>;
    fn contains(&self, file_key: &str) -> bool;
    fn save_loudness(&self, file_key: &str, loudness: i32) -> RepoResult<()>;
    /// Mark the file as analysed but without a usable loudness value.
    fn save_unavailable(&self, file_key: &str) -> RepoResult<()>;
    fn count_analysed(&self) -> usize;
    fn flush(&self);
    fn delete_all(&self);
}

pub type ArcLoudnessRepository = Arc<dyn LoudnessRepository>;
