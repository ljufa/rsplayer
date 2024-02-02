use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{self, Duration},
};

use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use log::{info, warn};
use sled::Db;
use symphonia::core::{
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::{MetadataOptions, StandardTagKey, Tag, Visual},
    probe::{Hint, ProbeResult},
};
use tokio::sync::broadcast::Sender;
use walkdir::WalkDir;

use api_models::{
    common::{to_database_key, MetadataLibraryItem},
    player::Song,
    settings::MetadataStoreSettings,
    stat::PlayItemStatistics,
    state::StateChangeEvent,
};

use crate::song_repository::SongRepository;
use crate::{album_repository::AlbumRepository, play_statistic_repository::PlayStatisticsRepository};

const ARTWORK_DIR: &str = "artwork";

pub struct MetadataService {
    ignored_files_db: Db,
    pub settings: MetadataStoreSettings,
    scan_running: AtomicBool,
    song_repository: Arc<SongRepository>,
    album_repository: Arc<AlbumRepository>,
    statistic_repository: Arc<PlayStatisticsRepository>,
}

impl MetadataService {
    pub fn new(
        settings: &MetadataStoreSettings,
        song_repository: Arc<SongRepository>,
        album_repository: Arc<AlbumRepository>,
        statistic_repository: Arc<PlayStatisticsRepository>,
    ) -> Result<Self> {
        let settings = settings.clone();
        let ignored_files_db = sled::open(&settings.db_path)?;
        Ok(Self {
            ignored_files_db,
            settings,
            scan_running: AtomicBool::new(false),
            song_repository,
            album_repository,
            statistic_repository,
        })
    }

    pub fn like_media_item(&self, media_item_id: &str) {
        self.update_or_create_media_item_stat(media_item_id, |item| item.liked_count += 1);
    }

    pub fn dislike_media_item(&self, media_item_id: &str) {
        self.update_or_create_media_item_stat(media_item_id, |item| item.liked_count -= 1);
    }

    pub fn increase_play_count(&self, media_item_id: &str) {
        self.update_or_create_media_item_stat(media_item_id, |item| item.play_count += 1);
    }

    fn update_or_create_media_item_stat<J>(&self, media_item_id: &str, mut job: J)
    where
        J: FnMut(&mut PlayItemStatistics),
    {
        if let Some(mut stat) = self.statistic_repository.find_by_id(media_item_id) {
            job(&mut stat);
            self.statistic_repository.save(&stat);
        } else {
            let mut stat = PlayItemStatistics {
                play_item_id: media_item_id.to_string(),
                ..Default::default()
            };
            job(&mut stat);
            self.statistic_repository.save(&stat);
        };
    }

    pub fn search_local_files_by_dir(&self, dir: &str) -> Vec<MetadataLibraryItem> {
        let start_time = std::time::Instant::now();
        let result = self.song_repository.find_by_key_prefix(dir).map(|(key, value)| {
            let key = String::from_utf8(key.to_vec()).unwrap();
            let Some((_, right)) = key.split_once(dir) else {
                return MetadataLibraryItem::Empty;
            };
            if right.contains('/') {
                let Some((left, _)) = right.split_once('/') else {
                    return MetadataLibraryItem::Empty;
                };
                MetadataLibraryItem::Directory { name: left.to_owned() }
            } else {
                MetadataLibraryItem::SongItem(Song::bytes_to_song(&value).expect(
                    "Failed to
                         convert bytes to song",
                ))
            }
        });
        let mut unique: Vec<MetadataLibraryItem> = result.collect();
        unique.dedup();
        log::info!("search_local_files_by_dir took {:?}", start_time.elapsed());
        unique
    }

    pub fn search_local_files_by_dir_contains(&self, search_term: &str, limit: usize) -> Vec<MetadataLibraryItem> {
        let start_time = std::time::Instant::now();
        let result = self
            .song_repository
            .find_by_key_contains(search_term)
            .map(|(key, value)| {
                let key = String::from_utf8(key.to_vec()).unwrap();
                if let Some((path, _)) = key.rsplit_once('/') {
                    if path.to_lowercase().contains(search_term.to_lowercase().as_str()) {
                        MetadataLibraryItem::Directory { name: path.to_owned() }
                    } else {
                        MetadataLibraryItem::SongItem(
                            Song::bytes_to_song(&value).expect("Failed to convert bytes to song"),
                        )
                    }
                } else {
                    MetadataLibraryItem::Empty
                }
            })
            .take(limit);
        let mut unique: Vec<MetadataLibraryItem> = result.collect();
        unique.dedup();
        log::info!("search_local_files_by_dir_contains took {:?}", start_time.elapsed());
        unique
    }

    pub fn scan_music_dir(&self, full_scan: bool, state_changes_sender: &Sender<StateChangeEvent>) {
        if self.scan_running.load(Ordering::SeqCst) {
            return;
        }
        self.scan_running.store(true, Ordering::SeqCst);
        let start_time = time::Instant::now();
        state_changes_sender
            .send(StateChangeEvent::MetadataSongScanStarted)
            .expect("msg send error");
        if full_scan {
            self.song_repository.delete_all();
        }

        if !Path::new(ARTWORK_DIR).exists() {
            std::fs::create_dir(ARTWORK_DIR).expect("Failed to create artwork directory");
        }
        let (new_files, deleted_db_keys) = self.get_diff();
        let count = self.add_songs_to_db(new_files, state_changes_sender);

        if !full_scan {
            info!("Deleting {} files from database", deleted_db_keys.len());
            for db_key in &deleted_db_keys {
                self.song_repository.delete(db_key);
                state_changes_sender
                    .send(StateChangeEvent::MetadataSongScanned(format!(
                        "Key {db_key} deleted from database"
                    )))
                    .expect("Status send failed");
            }
        }
        state_changes_sender
            .send(StateChangeEvent::MetadataSongScanFinished(format!(
                "Music directory scan finished: {count} files scanned in {} seconds",
                start_time.elapsed().as_secs()
            )))
            .expect("Status send failed");
        info!(
            "Scanning of {} files finished in {}s",
            count,
            start_time.elapsed().as_secs()
        );
        self.scan_running.store(false, Ordering::SeqCst);
    }

    fn full_path_to_database_key(&self, input: &str) -> String {
        let mut music_dir_pref = self.settings.music_directory.clone();
        if !music_dir_pref.ends_with('/') {
            music_dir_pref.push('/');
        }
        let file_path = input.replace(&music_dir_pref, "");
        to_database_key(file_path.as_str())
    }

    fn get_diff(&self) -> (Vec<String>, Vec<String>) {
        let mut added_files: Vec<String> = Vec::new();
        let mut deleted_keys: Vec<String> = Vec::new();
        let mut unchanged_keys: Vec<String> = Vec::new();

        for entry in WalkDir::new(&self.settings.music_directory)
            .follow_links(self.settings.follow_links)
            .sort_by_file_name()
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|de| de.file_type().is_file())
            .filter(|de| {
                let ext = de
                    .path()
                    .extension()
                    .map_or("no_ext".to_string(), |ex| ex.to_str().unwrap().to_lowercase());
                self.settings.supported_extensions.contains(&ext)
            })
            .map(|de| {
                (
                    de.path().to_str().unwrap().to_owned(),
                    self.full_path_to_database_key(de.path().to_str().unwrap()),
                )
            })
            .filter(|de| !self.ignored_files_db.contains_key(&de.1).unwrap_or(false))
        {
            if self.song_repository.find_by_id(&entry.1).is_some() {
                unchanged_keys.push(entry.1.clone());
            } else {
                added_files.push(entry.0.clone());
            }
        }
        for song in self.song_repository.get_all_iterator() {
            if !unchanged_keys.contains(&song.file) {
                deleted_keys.push(song.file);
            }
        }
        (added_files, deleted_keys)
    }

    fn add_songs_to_db(&self, files: Vec<String>, state_changes_sender: &Sender<StateChangeEvent>) -> u32 {
        let mut count = 0;
        for file in files {
            state_changes_sender
                .send(StateChangeEvent::MetadataSongScanned(format!(
                    "Scanning: {count}. {file}"
                )))
                .expect("Status send failed");
            {
                if let Err(e) = self.scan_single_file(Path::new(&file)) {
                    self.ignored_files_db
                        .insert(self.full_path_to_database_key(&file), e.to_string().as_bytes())
                        .expect("DB error");
                }
            }
            if count % 100 == 0 {
                self.song_repository.flush();
            }
            count += 1;
        }
        self.song_repository.flush();
        _ = self.ignored_files_db.flush();
        count
    }

    fn scan_single_file(&self, file_path: &Path) -> Result<()> {
        info!("Scanning file:\t{:?}", file_path);

        let file = Box::new(File::open(file_path).unwrap());
        let file_modification_date: DateTime<Utc> = file.as_ref().metadata()?.modified()?.into();

        let mss = MediaSourceStream::new(file, MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = file_path.extension() {
            let ext = ext.to_str().unwrap().to_lowercase();
            hint.with_extension(&ext);
        }
        let format_opts = FormatOptions {
            enable_gapless: false,
            ..Default::default()
        };
        // Use the default options for metadata readers.
        let metadata_opts = MetadataOptions::default();
        let file_p = &self.full_path_to_database_key(file_path.to_str().unwrap());

        info!("Scanning file:\t{}", file_p);
        match symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
            Ok(mut probed) => {
                let (mut song, image_data) = build_song(&mut probed);

                if let Some(image_data) = &image_data {
                    let image_id = uuid::Uuid::new_v4();
                    if let Err(e) = std::fs::write(Path::new(ARTWORK_DIR).join(image_id.to_string()), &image_data.data)
                    {
                        warn!("Error writing image file: {}", e);
                    } else {
                        song.image_id = Some(image_id.to_string());
                    }
                };

                song.file = file_p.to_string();
                song.file_date = file_modification_date;
                log::debug!("Add/update song in database: {:?}", song);
                self.song_repository.save(&song);
                self.album_repository.update_from_song(song);

                Ok(())
            }
            Err(err) => {
                warn!("Error:{} {}", file_p, err);
                Err(Error::msg(format!("Error:{file_p} {err}")))
            }
        }
    }
}

fn build_song(probed: &mut ProbeResult) -> (Song, Option<Visual>) {
    let mut song = Song::default();
    let mut image_data: Option<Visual> = None;
    if let Some(track) = probed.format.default_track() {
        let params = &track.codec_params;
        if let Some(n_frames) = params.n_frames {
            if let Some(tb) = params.time_base {
                let time = tb.calc_time(n_frames);
                song.time = Some(Duration::from_secs(time.seconds));
            }
        }
    }
    if let Some(metadata_rev) = probed.format.metadata().current() {
        let tags = metadata_rev.tags();
        for known_tag in tags.iter().filter(|t| t.is_known()) {
            match known_tag.std_key.unwrap_or(StandardTagKey::Version) {
                StandardTagKey::Album => song.album = from_tag_value_to_option(known_tag),
                StandardTagKey::AlbumArtist => {
                    song.album_artist = from_tag_value_to_option(known_tag);
                }
                StandardTagKey::Artist => song.artist = from_tag_value_to_option(known_tag),
                StandardTagKey::Composer => song.composer = from_tag_value_to_option(known_tag),
                StandardTagKey::Date => song.date = from_tag_value_to_option(known_tag),
                StandardTagKey::DiscNumber => song.disc = from_tag_value_to_option(known_tag),
                StandardTagKey::Genre => song.genre = from_tag_value_to_option(known_tag),
                StandardTagKey::Label => song.label = from_tag_value_to_option(known_tag),
                StandardTagKey::Performer => song.performer = from_tag_value_to_option(known_tag),
                StandardTagKey::TrackNumber => song.track = from_tag_value_to_option(known_tag),
                StandardTagKey::TrackTitle => {
                    song.title = from_tag_value_to_option(known_tag);
                }
                _ => {}
            }
        }
        for unknown_tag in tags.iter().filter(|t| !t.is_known()) {
            song.tags.insert(
                unknown_tag.key.clone(),
                from_tag_value_to_option(unknown_tag).unwrap_or_default(),
            );
        }
        if let Some(v) = metadata_rev.visuals().iter().next() {
            image_data = Some(v.clone());
        }
    }
    (song, image_data)
}

#[allow(clippy::unnecessary_wraps)]
fn from_tag_value_to_option(tag: &Tag) -> Option<String> {
    Some(tag.value.to_string())
}
