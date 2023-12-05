use std::{
    fs::File,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{self, Duration},
};

use anyhow::{Error, Result};
use log::{info, warn};
use sled::Db;
use symphonia::core::{
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::{MetadataOptions, StandardTagKey, Tag},
    probe::{Hint, ProbeResult},
};
use tokio::sync::broadcast::Sender;
use walkdir::WalkDir;

use api_models::{
    common::to_database_key, player::Song, settings::MetadataStoreSettings, state::StateChangeEvent,
};

pub struct LocalLibrary {
    db: Db,
    settings: MetadataStoreSettings,
    scan_running: AtomicBool,
}

impl LocalLibrary {
    pub fn new(settings: &MetadataStoreSettings) -> Result<Self> {
        let settings = settings.clone();
        let db = sled::open(settings.db_path.as_str())?;
        Ok(Self {
            db,
            settings,
            scan_running: AtomicBool::new(false),
        })
    }
    pub fn find_song_by_id(&self, song_id: &str) -> Option<Song> {
        self.get_all_songs_iterator().find(|s| s.id == song_id)
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
        let excluded_db = self.db.open_tree("excluded").expect("DB error");

        for entry in WalkDir::new(&self.settings.music_directory)
            .follow_links(self.settings.follow_links)
            .sort_by_file_name()
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|de| de.file_type().is_file())
            .filter(|de| {
                let ext = de.path().extension().map_or("no_ext".to_string(), |ex| {
                    ex.to_str().unwrap().to_lowercase()
                });
                self.settings.supported_extensions.contains(&ext)
            })
            .map(|de| {
                (
                    de.path().to_str().unwrap().to_owned(),
                    self.full_path_to_database_key(de.path().to_str().unwrap()),
                )
            })
            .filter(|de| !excluded_db.contains_key(&de.1).unwrap_or(false))
        {
            if self.db.contains_key(entry.1.as_bytes()).unwrap_or(false) {
                unchanged_keys.push(entry.1.clone());
            } else {
                added_files.push(entry.0.clone());
            }
        }
        for entry in self.db.iter().filter_map(Result::ok) {
            let iveckey = entry.0.to_vec();
            let key = String::from_utf8_lossy(&iveckey);
            if !unchanged_keys.contains(&key.to_string()) {
                deleted_keys.push(key.into_owned());
            }
        }
        (added_files, deleted_keys)
    }

    pub fn scan_music_dir(&self, full_scan: bool, state_changes_sender: &Sender<StateChangeEvent>) {
        if self.scan_running.load(Ordering::SeqCst) {
            return;
        }
        self.scan_running.store(true, Ordering::SeqCst);
        let start_time = time::Instant::now();
        state_changes_sender.send(StateChangeEvent::MetadataSongScanStarted).expect("msg send error");
        if full_scan {
            _ = self.db.clear();
        }
        let (new_files, deleted_db_keys) = self.get_diff();
        let count = self.add_songs_to_db(new_files, state_changes_sender);

        if !full_scan {
            info!("Deleting {} files from database", deleted_db_keys.len());
            for db_key in &deleted_db_keys {
                self.db.remove(db_key).expect("Delete failed");
                state_changes_sender
                    .send(StateChangeEvent::MetadataSongScanned(
                        format!("Key {db_key} deleted from database"),
                    ))
                    .expect("Status send failed");
                _ = self.db.flush();
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

    fn add_songs_to_db(
        &self,
        files: Vec<String>,
        state_changes_sender: &Sender<StateChangeEvent>,
    ) -> u32 {
        let excluded_db = self.db.open_tree("excluded").expect("DB error");
        let mut count = 0;
        for file in files {
            state_changes_sender
                .send(StateChangeEvent::MetadataSongScanned(format!(
                    "Scanning: {count}. {file}"
                )))
                .expect("Status send failed");
            if let Err(e) = self.scan_single_file(Path::new(&file)) {
                excluded_db
                    .insert(self.full_path_to_database_key(&file), e.to_string().as_bytes())
                    .expect("DB error");
            }
            if count % 100 == 0 {
                _ = self.db.flush();
            }
            count += 1;
        }
        _ = self.db.flush();
        _ = excluded_db.flush();
        count
    }

    fn scan_single_file(&self, file_path: &Path) -> Result<()> {
        info!("Scanning file:\t{:?}", file_path);
        let file = Box::new(File::open(file_path).unwrap());
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
                let mut song = build_song(&mut probed);

                song.id = self
                    .db
                    .generate_id()
                    .expect("failed to generate id")
                    .to_string();
                song.file = file_p.to_string();
                log::debug!("Add/update song in database: {:?}", song);
                _ = self.db.insert(file_p, song.to_json_string_bytes());
                Ok(())
            }
            Err(err) => {
                warn!("Error:{} {}", file_p, err);
                Err(Error::msg(format!("Error:{file_p} {err}")))
            }
        }
    }
    pub fn get_all_songs_iterator(&self) -> impl Iterator<Item = Song> {
        self.db
            .iter()
            .filter_map(std::result::Result::ok)
            .map_while(|s| Song::bytes_to_song(&s.1))
    }
}

fn build_song(probed: &mut ProbeResult) -> Song {
    let mut song = Song::default();
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
                    let title = from_tag_value_to_option(known_tag);
                    song.title = title;
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
    }
    song
}

#[allow(clippy::unnecessary_wraps)]
fn from_tag_value_to_option(tag: &Tag) -> Option<String> {
    Some(tag.value.to_string())
}
