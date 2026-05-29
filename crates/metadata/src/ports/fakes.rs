//! In-memory test doubles for the repository ports.
//!
//! Use these in unit tests to exercise services without a fjall database.

use std::sync::Mutex;

use api_models::{player::Song, playlist::Album, stat::PlayItemStatistics};

use crate::error::{RepoError, RepoResult};
use crate::ports::{
    album_repository::AlbumRepository, loudness_repository::LoudnessRepository,
    play_statistics_repository::PlayStatisticsRepository, song_repository::SongRepository,
};

#[derive(Default)]
pub struct InMemorySongRepository {
    songs: Mutex<Vec<Song>>,
}

impl SongRepository for InMemorySongRepository {
    fn save(&self, song: &Song) -> RepoResult<()> {
        if song.file.is_empty() {
            return Err(RepoError::Invalid("empty file key".to_owned()));
        }
        let mut g = self.songs.lock().unwrap();
        if let Some(existing) = g.iter_mut().find(|s| s.file == song.file) {
            *existing = song.clone();
        } else {
            g.push(song.clone());
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> RepoResult<()> {
        let mut g = self.songs.lock().unwrap();
        g.retain(|s| s.file != id);
        Ok(())
    }

    fn delete_all(&self) {
        self.songs.lock().unwrap().clear();
    }

    fn find_by_id(&self, id: &str) -> Option<Song> {
        self.songs.lock().unwrap().iter().find(|s| s.file == id).cloned()
    }

    fn find_all(&self) -> Vec<Song> {
        self.songs.lock().unwrap().clone()
    }

    fn find_by_key_contains(&self, search_term: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        let st = search_term.to_lowercase();
        self.songs
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.file.to_lowercase().contains(&st))
            .map(|s| (s.file.as_bytes().to_vec(), s.to_json_string_bytes()))
            .collect()
    }

    fn find_by_key_prefix(&self, prefix: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.songs
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.file.starts_with(prefix))
            .map(|s| (s.file.as_bytes().to_vec(), s.to_json_string_bytes()))
            .collect()
    }

    fn find_songs_by_dir_prefix(&self, prefix: &str) -> Vec<Song> {
        self.songs
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.file.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn flush(&self) {}
}

#[derive(Default)]
pub struct InMemoryAlbumRepository {
    albums: Mutex<Vec<Album>>,
}

impl AlbumRepository for InMemoryAlbumRepository {
    fn delete_all(&self) {
        self.albums.lock().unwrap().clear();
    }

    fn find_all(&self) -> Vec<Album> {
        self.albums.lock().unwrap().clone()
    }

    fn find_all_album_artists(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .albums
            .lock()
            .unwrap()
            .iter()
            .filter_map(|a| a.artist.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn find_by_id(&self, album_id: &str) -> Option<Album> {
        self.albums.lock().unwrap().iter().find(|a| a.id == album_id).cloned()
    }

    fn find_all_sort_by_added_desc(&self, limit: usize) -> Vec<Album> {
        let mut all = self.find_all();
        all.sort_by(|a, b| b.added.cmp(&a.added));
        all.truncate(limit);
        all
    }

    fn find_all_sort_by_released_desc(&self, limit: usize) -> Vec<Album> {
        let mut all = self.find_all();
        all.sort_by(|a, b| b.released.cmp(&a.released));
        all.truncate(limit);
        all
    }

    fn find_all_by_genre(&self, _limit_per_genre: usize) -> Vec<(String, Vec<Album>)> {
        Vec::new()
    }

    fn find_all_by_decade(&self, _limit_per_decade: usize) -> Vec<(String, Vec<Album>)> {
        Vec::new()
    }

    fn find_by_artist(&self, artist: &str) -> Vec<Album> {
        self.albums
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.artist.as_deref() == Some(artist))
            .cloned()
            .collect()
    }

    fn update_from_song(&self, song: Song) -> RepoResult<()> {
        let title = song.album.clone().unwrap_or_default();
        let artist = song.album_artist.clone().or_else(|| song.artist.clone());
        let key = format!("{}|{}", artist.clone().unwrap_or_default(), title);
        let mut g = self.albums.lock().unwrap();
        if let Some(existing) = g.iter_mut().find(|a| a.id == key) {
            if !existing.song_keys.contains(&song.file) {
                existing.song_keys.push(song.file);
            }
        } else {
            g.push(Album {
                id: key,
                title,
                artist,
                song_keys: vec![song.file],
                ..Default::default()
            });
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryPlayStatisticsRepository {
    stats: Mutex<Vec<PlayItemStatistics>>,
}

impl PlayStatisticsRepository for InMemoryPlayStatisticsRepository {
    fn find_by_id(&self, play_item_id: &str) -> Option<PlayItemStatistics> {
        self.stats
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.play_item_id == play_item_id)
            .cloned()
    }

    fn find_by_key_prefix(&self, prefix: &str) -> Vec<PlayItemStatistics> {
        self.stats
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.play_item_id.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn get_all(&self) -> Vec<PlayItemStatistics> {
        self.stats.lock().unwrap().clone()
    }

    fn save(&self, play_item_statistics: &PlayItemStatistics) -> RepoResult<()> {
        let mut g = self.stats.lock().unwrap();
        if let Some(existing) = g
            .iter_mut()
            .find(|s| s.play_item_id == play_item_statistics.play_item_id)
        {
            *existing = play_item_statistics.clone();
        } else {
            g.push(play_item_statistics.clone());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryLoudnessRepository {
    entries: Mutex<Vec<(String, Option<i32>)>>,
}

impl LoudnessRepository for InMemoryLoudnessRepository {
    fn get(&self, file_key: &str) -> Option<i32> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|(k, _)| k == file_key)
            .and_then(|(_, v)| *v)
    }

    fn contains(&self, file_key: &str) -> bool {
        self.entries.lock().unwrap().iter().any(|(k, _)| k == file_key)
    }

    fn save_loudness(&self, file_key: &str, loudness: i32) -> RepoResult<()> {
        let mut g = self.entries.lock().unwrap();
        if let Some(existing) = g.iter_mut().find(|(k, _)| k == file_key) {
            existing.1 = Some(loudness);
        } else {
            g.push((file_key.to_owned(), Some(loudness)));
        }
        Ok(())
    }

    fn save_unavailable(&self, file_key: &str) -> RepoResult<()> {
        let mut g = self.entries.lock().unwrap();
        if let Some(existing) = g.iter_mut().find(|(k, _)| k == file_key) {
            existing.1 = None;
        } else {
            g.push((file_key.to_owned(), None));
        }
        Ok(())
    }

    fn count_analysed(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    fn flush(&self) {}

    fn delete_all(&self) {
        self.entries.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_round_trip() {
        let repo = InMemorySongRepository::default();
        let song = Song {
            file: "a/b.flac".to_owned(),
            title: Some("Track".to_owned()),
            ..Default::default()
        };
        repo.save(&song).expect("save");
        assert_eq!(repo.find_all().len(), 1);
        assert_eq!(repo.find_by_id("a/b.flac").unwrap().title.as_deref(), Some("Track"));
        assert_eq!(repo.find_by_key_prefix("a/").len(), 1);
        assert_eq!(repo.find_songs_by_dir_prefix("a/").len(), 1);
        repo.delete("a/b.flac").expect("delete");
        assert!(repo.find_all().is_empty());
    }

    #[test]
    fn album_round_trip() {
        let repo = InMemoryAlbumRepository::default();
        let song = Song {
            file: "f.flac".to_owned(),
            album: Some("X".to_owned()),
            artist: Some("Y".to_owned()),
            ..Default::default()
        };
        repo.update_from_song(song).expect("update");
        let albums = repo.find_by_artist("Y");
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].title, "X");
    }

    #[test]
    fn play_stats_round_trip() {
        let repo = InMemoryPlayStatisticsRepository::default();
        let stat = PlayItemStatistics {
            play_item_id: "id1".to_owned(),
            play_count: 3,
            ..Default::default()
        };
        repo.save(&stat).expect("save");
        assert_eq!(repo.find_by_id("id1").unwrap().play_count, 3);
        let updated = PlayItemStatistics {
            play_item_id: "id1".to_owned(),
            play_count: 4,
            ..Default::default()
        };
        repo.save(&updated).expect("save");
        assert_eq!(repo.find_by_id("id1").unwrap().play_count, 4);
        assert_eq!(repo.get_all().len(), 1);
    }

    #[test]
    fn loudness_round_trip() {
        let repo = InMemoryLoudnessRepository::default();
        repo.save_loudness("a", 1234).expect("save loudness");
        assert!(repo.contains("a"));
        assert_eq!(repo.get("a"), Some(1234));
        repo.save_unavailable("b").expect("save unavailable");
        assert!(repo.contains("b"));
        assert_eq!(repo.get("b"), None);
        assert_eq!(repo.count_analysed(), 2);
    }
}
