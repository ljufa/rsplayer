use api_models::{player::Song, settings::PlaybackQueueSetting};
use log::trace;
use sled::{Db, IVec};

pub struct PlaybackQueue {
    db: Db,
    current_song_id: Option<IVec>,
    queue: Vec<IVec>,
}

impl PlaybackQueue {
    pub fn new(settings: &PlaybackQueueSetting) -> Self {
        let db = sled::open(&settings.db_path).expect("Failed to open queue db");
        let queue: Vec<IVec> = db
            .iter()
            .filter_map(std::result::Result::ok)
            .map(|item| item.0)
            .collect();

        PlaybackQueue {
            db,
            current_song_id: queue.first().cloned(),
            queue,
        }
    }

    pub fn get_current_song(&self) -> Option<Song> {
        if let Some(song_id) = self.get_current_or_first_song_id() {
            if let Ok(Some(value)) = self.db.get(song_id) {
                return Song::bytes_to_song(value.to_vec());
            }
        }
        None
    }

    pub fn move_current_to_next_song(&mut self) -> bool {
        let Some(current) = self.get_current_or_first_song_id() else {
            return false;
        };
        let mut iter = self
            .queue
            .iter()
            .skip_while(|el| el.to_vec() != current.to_vec())
            .skip(1);
        let Some(next) = iter.next() else {
            return false;
        };
        if self.current_song_id.as_ref() == Some(next) {
            self.current_song_id = None;
            return false;
        }
        self.current_song_id = Some(next.clone());
        trace_print_key(self.current_song_id.as_ref().unwrap(), "Next current key=");
        true
    }

    pub fn add_song(&mut self, song: Song) {
        let key = IVec::from(song.file.as_bytes());
        trace_print_key(&key, "Add key=");
        self.db
            .insert(&key, song.to_json_string_bytes())
            .expect("Failed to add song to the queue database");
        self.queue.push(key);
    }

    fn get_current_or_first_song_id(&self) -> Option<IVec> {
        let result = if self.current_song_id.is_some() {
            self.current_song_id.clone()
        } else if !self.queue.is_empty() {
            self.queue.first().cloned()
        } else if !self.db.is_empty() {
            if let Ok(Some(first)) = self.db.last() {
                Some(first.0)
            } else {
                None
            }
        } else {
            None
        };
        if result.is_some() {
            trace_print_key(result.as_ref().unwrap(), "Current key is=");
        }
        result
    }

    pub fn replace_all(&mut self, iter: impl Iterator<Item = Option<Song>>) {
        _ = self.db.clear();
        self.queue.clear();
        iter.for_each(|song| {
            if let Some(song) = song.as_ref() {
                let key = IVec::from(song.file.clone().as_bytes());
                _ = self.db.insert(&key, song.to_json_string_bytes());
                self.queue.push(key)
            }
        });
    }
    pub fn get_queue_page(&mut self, offset: usize, limit: usize) -> (usize, Vec<Song>) {
        let total = self.db.len();
        let from = self
            .queue
            .get(offset)
            .unwrap_or_else(|| self.queue.first().unwrap());
        trace_print_key(from, "From=");
        (
            total,
            self.db
                .range(from.to_vec()..)
                .filter_map(std::result::Result::ok)
                .take(limit).map_while(|s| Song::bytes_to_song(s.1.to_vec()))
                .collect(),
        )
    }
    pub fn get_all_songs(&mut self) -> Vec<Song> {
        self.db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| Song::bytes_to_song(s.1.to_vec()))
            .collect()
    }
}
fn trace_print_key(key: &IVec, msg: &str) {
    trace!(
        "{}{}",
        msg,
        String::from_utf8(key.to_vec()).expect("Invalid utf8 in key")
    );
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::*;
    use api_models::{common::hash_md5, player::Song, settings::PlaybackQueueSetting};

    #[test]
    fn should_replace_queue_with_new_songs() {
        let mut queue = create_queue();
        for ext in 0..10 {
            queue.add_song(create_song(format!("ext{ext}").as_str()));
        }
        assert_eq!(queue.db.len(), 10);

        let mut new_songs = Vec::new();
        for ext in 11..15 {
            new_songs.push(Some(create_song(format!("2ext{ext}").as_str())));
        }
        queue.replace_all(new_songs.iter().cloned());
        assert_eq!(queue.db.len(), 4);
        assert_eq!(
            queue.get_current_song().unwrap().file,
            "assets/music.2ext11"
        )
    }

    #[test]
    fn should_get_first_added_song_as_current() {
        let mut queue = create_queue();
        queue.add_song(create_song("mp3"));
        queue.add_song(create_song("wav"));
        queue.add_song(create_song("flac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
    }

    #[test]
    fn should_move_current_by_one() {
        let mut queue = create_queue();
        queue.add_song(create_song("mp3"));
        queue.add_song(create_song("flac"));
        queue.add_song(create_song("wav"));
        queue.add_song(create_song("aac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.wav");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.aac");
    }

    #[test]
    fn should_return_false_move_at_the_end() {
        let mut queue = create_queue();
        queue.add_song(create_song("mp3"));
        queue.add_song(create_song("flac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        // return false on last
        assert!(!queue.move_current_to_next_song());
    }

    #[test]
    fn should_return_all() {
        let mut queue = create_queue();
        for ext in 0..100 {
            queue.add_song(create_song(format!("ext{ext}").as_str()));
        }
        let all = queue.get_all_songs();
        assert_eq!(all.len(), 100);
        assert_eq!(all[0].file, "assets/music.ext0");
    }

    #[test]
    fn should_return_page() {
        let mut queue = create_queue();
        for ext in 'a'..='z' {
            queue.add_song(create_song(format!("{ext}").as_str()));
        }
        let (total, songs) = queue.get_queue_page(0, 10);
        assert_eq!(total, 26);
        assert_eq!(songs.len(), 10);
        assert_eq!(songs[0].file, "assets/music.a");
        assert_eq!(songs[9].file, "assets/music.j");
    }


    fn create_queue() -> PlaybackQueue {
        let ctx = Context::default();
        PlaybackQueue::new(&PlaybackQueueSetting {
            db_path: ctx.db_dir.clone(),
        })
    }
    fn create_song(ext: &str) -> Song {
        let file = format!("assets/music.{ext}");
        let id = hash_md5(&file);
        Song {
            id,
            file,
            ..Default::default()
        }
    }

    pub struct Context {
        pub db_dir: String,
    }

    impl Default for Context {
        fn default() -> Self {
            _ = env_logger::builder().is_test(true).try_init();
            let rnd = random_string::generate(6, "utf8");
            Self {
                db_dir: format!("/tmp/test_queue{rnd}"),
            }
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            let path = &self.db_dir;
            if Path::new(path).exists() {
                _ = std::fs::remove_dir_all(path);
            }
        }
    }
}
