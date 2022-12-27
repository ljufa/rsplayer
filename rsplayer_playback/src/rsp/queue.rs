use api_models::{player::Song, settings::PlaybackQueueSetting};
use log::{debug, trace};
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
        self.current_song_id = Some(next.clone());
        trace_print_key(self.current_song_id.as_ref().unwrap(), "Next current key=");
        true
    }

    pub fn add(&mut self, song: Song) {
        let key = IVec::from(song.id.as_bytes());
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
                let key = IVec::from(song.id.clone().as_bytes());
                _ = self.db.insert(&key, song.to_json_string_bytes());
                self.queue.push(key)
            }
        });
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
            queue.add(create_song(format!("ext{ext}").as_str()));
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
        queue.add(create_song("mp3"));
        queue.add(create_song("wav"));
        queue.add(create_song("flac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
    }

    #[test]
    fn should_move_current_by_one() {
        let mut queue = create_queue();
        queue.add(create_song("mp3"));
        queue.add(create_song("flac"));
        queue.add(create_song("wav"));
        queue.add(create_song("aac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.wav");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.aac");
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
