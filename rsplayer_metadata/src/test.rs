mod queue {
    use std::sync::Arc;

    use api_models::settings::PlaybackQueueSetting;

    use crate::song_repository::SongRepository;
    use crate::{
        queue_service::QueueService,
        test::test_shared::{create_song, create_song_with_title, Context},
    };

    #[test]
    fn should_replace_queue_with_new_songs() {
        let queue = create_queue();
        for ext in 0..10 {
            queue.add_song(&create_song(format!("ext{ext}").as_str()));
        }
        let all = queue.get_all_songs();
        assert_eq!(all.len(), 10);
        queue.move_current_to_next_song();
        let mut new_songs = Vec::new();
        for ext in 11..15 {
            new_songs.push(create_song(format!("2ext{ext}").as_str()));
        }

        queue.replace_all(new_songs.iter().cloned());
        let all = queue.get_all_songs();
        assert_eq!(all.len(), 4);
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.2ext11");
    }

    #[test]
    fn should_get_first_added_song_as_current() {
        let queue = create_queue();
        queue.add_song(&create_song("mp3"));
        queue.add_song(&create_song("wav"));
        queue.add_song(&create_song("flac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
    }

    #[test]
    fn should_move_current_to_next_by_one() {
        let queue = create_queue();
        queue.add_song(&create_song("mp3"));
        queue.add_song(&create_song("flac"));
        queue.add_song(&create_song("wav"));
        queue.add_song(&create_song("aac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.wav");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.aac");
    }

    #[test]
    fn should_move_current_to_prev_by_one() {
        let queue = create_queue();
        queue.add_song(&create_song("mp3"));
        queue.add_song(&create_song("flac"));
        queue.add_song(&create_song("wav"));
        queue.add_song(&create_song("aac"));

        assert!(queue.move_current_to_next_song());
        assert!(queue.move_current_to_next_song());
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.aac");
        assert!(queue.move_current_to_previous_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.wav");
        assert!(queue.move_current_to_previous_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        assert!(queue.move_current_to_previous_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
    }

    #[test]
    fn should_remove_song() {
        let queue = create_queue();
        queue.add_song(&create_song("aac"));
        queue.add_song(&create_song("flac"));
        queue.add_song(&create_song("mp3"));
        let all_songs = queue.get_all_songs();
        assert_eq!(all_songs[0].file, "assets/music.aac");
        queue.remove_song(&all_songs[0].file);
        let all_songs = queue.get_all_songs();
        assert_eq!(all_songs.len(), 2);
        assert_eq!(all_songs[0].file, "assets/music.flac");
    }

    #[test]
    fn should_return_false_move_at_the_end() {
        let queue = create_queue();
        queue.add_song(&create_song("mp3"));
        queue.add_song(&create_song("flac"));
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.mp3");
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        // return false on last
        assert!(!queue.move_current_to_next_song());
    }

    #[test]
    fn should_move_current_to_specific_song_id() {
        let queue = create_queue();
        for ext in 'a'..='z' {
            queue.add_song(&create_song(format!("{ext}").as_str()));
        }
        let all_songs = queue.get_all_songs();
        let song_10 = &all_songs[9];
        assert!(queue.move_current_to(&song_10.file));
        assert_eq!(queue.get_current_song().unwrap().file, song_10.file);

        let song_15 = &all_songs[14];
        assert!(queue.move_current_to(&song_15.file));
        assert_eq!(queue.get_current_song().unwrap().file, song_15.file);
    }

    #[test]
    fn should_return_all() {
        let queue = create_queue();
        for ext in 0..100 {
            queue.add_song(&create_song(format!("ext{ext}").as_str()));
        }
        let all = queue.get_all_songs();
        assert_eq!(all.len(), 100);
        assert_eq!(all[0].file, "assets/music.ext0");
    }

    #[test]
    fn should_return_page_no_filter() {
        let queue = create_queue();
        for ext in 'a'..='z' {
            queue.add_song(&create_song(format!("{ext}").as_str()));
        }
        let (total, songs) = queue.get_queue_page(0, 10, |_| true);
        assert_eq!(total, 26);
        assert_eq!(songs.len(), 10);
        assert_eq!(songs[0].file, "assets/music.a");
        assert_eq!(songs[9].file, "assets/music.j");
    }

    #[test]
    fn should_return_page_with_filter() {
        let queue = create_queue();
        queue.add_song(&create_song_with_title("title1"));
        queue.add_song(&create_song_with_title("title2"));
        queue.add_song(&create_song_with_title("title10"));
        queue.add_song(&create_song_with_title("bye title1"));
        queue.add_song(&create_song_with_title("bye title2"));
        let (total, songs) = queue.get_queue_page(0, 20, |song| song.get_title().contains("title1"));
        assert_eq!(total, 5);
        assert_eq!(songs.len(), 3);
    }

    #[test]
    fn should_return_page_starting_from_current_page() {
        let queue = create_queue();
        queue.add_song(&create_song_with_title("title1"));
        queue.add_song(&create_song_with_title("title2"));
        queue.add_song(&create_song_with_title("title10"));
        queue.add_song(&create_song_with_title("bye title1"));
        queue.add_song(&create_song_with_title("bye title2"));
        let page = queue.get_queue_page_starting_from_current_song(10);
        assert_eq!(page.len(), 5);
        let page = queue.get_queue_page_starting_from_current_song(3);
        assert_eq!(page.len(), 3);
        queue.move_current_to_next_song();
        let page = queue.get_queue_page_starting_from_current_song(10);
        assert_eq!(page.len(), 4);
    }

    #[test]
    fn should_persist_current_song_after_drop() {
        let ctx = Context::default();
        let queue = create_queue_with_ctx(&ctx);
        queue.add_song(&create_song("mp3"));
        queue.add_song(&create_song("flac"));
        queue.add_song(&create_song("wav"));
        queue.add_song(&create_song("ape"));
        assert!(queue.move_current_to_next_song());
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
        drop(queue);
        let queue = create_queue_with_ctx(&ctx);
        assert_eq!(queue.get_current_song().unwrap().file, "assets/music.flac");
    }

    #[test]
    fn should_not_return_next_song_when_random_next() {
        let ctx = Context::default();
        let queue = create_queue_with_ctx(&ctx);
        for ext in 0..5000 {
            queue.add_song(&create_song(format!("{ext}").as_str()));
        }
        queue.toggle_random_next();
        queue.move_current_to_next_song();
        assert_ne!(queue.get_current_song().unwrap().file, "assets/music.1");
    }

    #[test]
    fn should_not_return_song_when_random_next_and_only_one_song() {
        let queue = create_queue();
        queue.toggle_random_next();
        queue.add_song(&create_song("mp3"));
        assert!(!queue.move_current_to_next_song());
        assert!(!queue.move_current_to_previous_song());
    }

    #[test]
    fn should_clear_queue() {
        let ctx = Context::default();
        let queue = create_queue_with_ctx(&ctx);
        for ext in 'a'..='z' {
            queue.add_song(&create_song(format!("{ext}").as_str()));
        }
        assert_eq!(queue.get_all_songs().len(), 26);
        queue.clear();
        assert_eq!(queue.get_all_songs().len(), 0);
        assert_eq!(queue.get_current_song(), None);
        drop(queue);
        let queue = create_queue_with_ctx(&ctx);
        assert_eq!(queue.get_all_songs().len(), 0);
        assert_eq!(queue.get_current_song(), None);
    }

    fn create_queue() -> QueueService {
        let ctx = Context::default();
        create_queue_with_ctx(&ctx)
    }

    fn create_queue_with_ctx(ctx: &Context) -> QueueService {
        let song_repo = Arc::new(SongRepository::new(&format!("{}_songrepo", ctx.db_dir)));

        QueueService::new(
            &PlaybackQueueSetting {
                db_path: format!("{}_queue", ctx.db_dir.clone()),
            },
            song_repo,
        )
    }
}

#[cfg(test)]
mod metadata {
    use std::{fs, path::Path, process::Command, sync::Arc, vec};

    use tokio::sync::broadcast::{Receiver, Sender};

    use api_models::{common::MetadataLibraryItem, settings::MetadataStoreSettings, state::StateChangeEvent};

    use crate::song_repository::SongRepository;
    use crate::{
        album_repository::{self},
        metadata_service::MetadataService,
        test::test_shared::Context,
    };

    #[test]
    fn should_scan_music_dir_first_time() {
        let (service, sender, _receiver, song_repository) = create_metadata_service(&Context::default());
        service.scan_music_dir(true, &sender);
        assert_eq!(song_repository.get_all_iterator().count(), 6);
        let result = song_repository.find_by_id("aa/aaa/music.flac");
        if let Some(saved_song) = result {
            assert_eq!(saved_song.artist, Some("Artist 1".to_owned()));
            assert_eq!(saved_song.title, Some("FlacTitle".to_owned()));
            assert_eq!(saved_song.album, Some("Album Two".to_owned()));
            assert_eq!(saved_song.genre, Some("Country".to_owned()));

            assert!(saved_song.time.is_some());
        } else {
            panic!("Assertion failed");
        }
    }

    #[test]
    fn should_incrementally_scan_music_dir_add_2_new_files() {
        let mut context = Context::default();
        std::fs::create_dir_all(&context.db_dir).expect("failed to create dir");
        context.music_dir = context.db_dir.clone();
        std::fs::create_dir_all(&context.music_dir).expect("failed to create dir");
        let (service, sender, mut reciever, song_repository) = create_metadata_service(&context);

        // copy content of assets into /tmp
        Command::new("cp")
            .arg("-r")
            .arg("assets")
            .arg(&context.music_dir)
            .spawn()
            .expect("failed to execute process")
            .wait()
            .expect("failed to wait");
        service.scan_music_dir(true, &sender);
        assert_eq!(song_repository.get_all_iterator().count(), 6);

        fs::copy(
            format!("{}/assets/music.wav", &context.music_dir),
            format!("{}/assets/aa/music_new.wav", &context.music_dir),
        )
        .expect("Failed to copy file");
        fs::copy(
            format!("{}/assets/aa/music.flac", &context.music_dir),
            format!("{}/assets/ab/music_new.flac", &context.music_dir),
        )
        .expect("Failed to copy file");
        fs::remove_file(format!("{}/assets/aa/aaa/music.flac", &context.music_dir)).expect("Failed to delete file");
        service.scan_music_dir(false, &sender);
        assert_eq!(song_repository.get_all_iterator().count(), 7);

        let mut events = vec![];
        while let Ok(ev) = reciever.try_recv() {
            events.push(ev);
        }
        assert_eq!(events.len(), 13);
        assert!(events.iter().any(|ev| match ev {
            StateChangeEvent::MetadataSongScanned(msg) => {
                msg.contains("music_new.wav") || msg.contains("music_new.flac") || msg.contains("deleted from database")
            }
            _ => false,
        }));
    }

    #[test]
    fn test_get_items_by_dir() {
        let (service, sender, _ee, song_repository) = create_metadata_service(&Context::default());
        service.scan_music_dir(true, &sender);
        let result = song_repository.find_by_dir("aa/");
        assert_eq!(result.root_path, "aa/");
        assert_eq!(result.items.len(), 3);
        assert_eq!(
            result.items[0],
            MetadataLibraryItem::Directory { name: "aaa".to_owned() }
        );
        match &result.items[1] {
            MetadataLibraryItem::SongItem(s) => assert_eq!(s.file, "aa/music.flac"),
            _ => panic!("Should be a song"),
        }
        let items = song_repository.find_by_dir("").items;
        assert_eq!(items[0], MetadataLibraryItem::Directory { name: "aa".to_owned() });
        assert_eq!(items[1], MetadataLibraryItem::Directory { name: "ab".to_owned() });
        assert_eq!(items[2], MetadataLibraryItem::Directory { name: "ac".to_owned() });
        match &items[3] {
            MetadataLibraryItem::SongItem(s) => assert_eq!(s.file, "music.wav"),
            _ => panic!("Should be a song"),
        }
    }

    #[test]
    fn should_get_song() {
        let (service, sender, _receiver, song_repository) = create_metadata_service(&Context::default());
        service.scan_music_dir(true, &sender);
        let song = song_repository.find_by_id("aa/music.m4a");
        assert!(song.is_some());
        assert_eq!(song.unwrap().file, "aa/music.m4a");
    }

    pub fn create_metadata_service(
        context: &Context,
    ) -> (
        MetadataService,
        Sender<StateChangeEvent>,
        Receiver<StateChangeEvent>,
        Arc<SongRepository>,
    ) {
        let path = &format!("{}_ams", context.db_dir.clone());
        if Path::new(path).exists() {
            _ = std::fs::remove_dir_all(path);
        }
        let settings = MetadataStoreSettings {
            db_path: path.to_owned(),
            music_directory: context.music_dir.clone(),
            ..Default::default()
        };
        let album_repository = Arc::new(album_repository::AlbumRepository::new(&format!(
            "{}_arp",
            context.db_dir
        )));
        let song_repository = Arc::new(SongRepository::new(&format!("{}_srp", context.db_dir)));
        let sender = tokio::sync::broadcast::channel(20).0;
        let receiver = sender.subscribe();
        (
            MetadataService::new(&settings, song_repository.clone(), album_repository)
                .expect("Failed to create service"),
            sender,
            receiver,
            song_repository,
        )
    }
}

mod playlist {
    use std::vec;

    use api_models::{player::Song, settings::PlaylistSetting};

    use crate::playlist_service::PlaylistService;

    use super::test_shared::{create_song, Context};

    #[test]
    fn should_save_new_playlist() {
        let svc = create_pl_service();
        svc.save_new_playlist("plista1", &create_songs(10));
        let plists = svc.get_playlists().items;
        assert_eq!(plists.len(), 1);
        if let api_models::playlist::PlaylistType::Saved(pl) = &plists[0] {
            assert_eq!(pl.name, "plista1");
        } else {
            panic!("Plist name is wrong");
        }
    }

    #[test]
    fn should_get_playlist_page_by_name() {
        let svc = create_pl_service();
        let playlist_name1 = "plist1";
        let playlist_name2 = "plist2";
        svc.save_new_playlist(playlist_name1, &create_songs(200));
        svc.save_new_playlist(playlist_name2, &create_songs(100));
        let pl1_page_2 = svc.get_playlist_page_by_name(playlist_name1, 10, 20);
        assert_eq!(pl1_page_2.total, 200);
        assert_eq!(pl1_page_2.items.len(), 20);
        let pl2_page_2 = svc.get_playlist_page_by_name(playlist_name2, 10, 10);
        assert_eq!(pl2_page_2.total, 100);
        assert_eq!(pl2_page_2.items.len(), 10);
    }

    fn create_songs(number_of_songs: usize) -> Vec<Song> {
        let mut songs = vec![];
        for ext in 0..number_of_songs {
            songs.push(create_song(format!("{ext}").as_str()));
        }
        songs
    }

    fn create_pl_service() -> PlaylistService {
        let ctx = Context::default();
        PlaylistService::new(&PlaylistSetting {
            db_path: ctx.db_dir.to_string(),
        })
    }
}

pub mod test_shared {
    use std::path::Path;

    use api_models::{common::to_database_key, player::Song};

    pub fn create_song(ext: &str) -> Song {
        let file = format!("assets/music.{ext}");
        let id = to_database_key(&file);
        Song {
            file: id,
            ..Default::default()
        }
    }

    pub fn create_song_with_title(title: &str) -> Song {
        let mut song = create_song(random_string::generate(3, "asdlkjhpoiuwergglmjh").as_str());
        song.title = Some(title.to_string());
        song
    }

    pub struct Context {
        pub db_dir: String,
        pub music_dir: String,
    }

    impl Default for Context {
        fn default() -> Self {
            let rnd = random_string::generate(12, "utf8");

            Self {
                db_dir: format!("/tmp/rsptest_{rnd}"),
                music_dir: "assets".to_owned(),
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
