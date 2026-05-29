pub mod album_repository;
pub mod loudness_repository;
pub mod play_statistics_repository;
pub mod song_repository;

#[cfg(test)]
pub mod fakes;

pub use album_repository::{AlbumRepository, ArcAlbumRepository};
pub use loudness_repository::{ArcLoudnessRepository, LoudnessRepository};
pub use play_statistics_repository::{ArcPlayStatisticsRepository, PlayStatisticsRepository};
pub use song_repository::{ArcSongRepository, SongRepository};
