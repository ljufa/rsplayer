use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time,
};

use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use fjall::{Database, PersistMode};
use log::{debug, info, warn};
use symphonia::core::{formats::probe::Hint, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions};
use tokio::sync::broadcast::Sender;
use walkdir::WalkDir;

use api_models::{
    common::{to_database_key, MetadataLibraryItem},
    player::Song,
    settings::MetadataStoreSettings,
    stat::{LibraryStats, PlayItemStatistics},
    state::StateChangeEvent,
};

use crate::audio_metadata_extractor::AudioMetadataExtractor;
use crate::sacd_bundle::{detect_sector_mode, read_areas, read_tracks, SACD_TRACK_MARKER};
use crate::song_repository::SongRepository;
use crate::{album_repository::AlbumRepository, play_statistic_repository::PlayStatisticsRepository};

const ARTWORK_DIR: &str = "artwork";

pub struct MetadataService {
    settings: RwLock<MetadataStoreSettings>,
    scan_running: AtomicBool,
    song_repository: Arc<SongRepository>,
    album_repository: Arc<AlbumRepository>,
    statistic_repository: Arc<PlayStatisticsRepository>,
    db: Arc<Database>,
}

impl MetadataService {
    pub fn new(
        db: Arc<Database>,
        settings: &MetadataStoreSettings,
        song_repository: Arc<SongRepository>,
        album_repository: Arc<AlbumRepository>,
        statistic_repository: Arc<PlayStatisticsRepository>,
    ) -> Result<Self> {
        let settings = settings.clone();

        Ok(Self {
            settings: RwLock::new(settings),
            scan_running: AtomicBool::new(false),
            song_repository,
            album_repository,
            statistic_repository,
            db,
        })
    }

    pub fn update_settings(&self, settings: MetadataStoreSettings) {
        *self.settings.write().expect("settings lock poisoned") = settings;
    }

    pub fn effective_directories(&self) -> Vec<String> {
        self.settings
            .read()
            .expect("settings lock poisoned")
            .effective_directories()
    }

    pub fn get_favorite_radio_stations(&self) -> Vec<String> {
        self.statistic_repository
            .find_by_key_prefix("radio_uuid_")
            .iter()
            .filter(|stat| stat.liked_count > 0)
            .map(|stat| {
                stat.play_item_id
                    .strip_prefix("radio_uuid_")
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    pub fn get_most_played_songs(&self, limit: usize) -> Vec<Song> {
        let mut stats = self.statistic_repository.get_all();
        stats.sort_by(|a, b| b.play_count.cmp(&a.play_count));
        stats
            .into_iter()
            .filter(|stat| !stat.play_item_id.starts_with("radio_uuid_"))
            .filter(|stat| stat.play_count > 0)
            .filter_map(|stat| self.song_repository.find_by_id(&stat.play_item_id))
            .take(limit)
            .collect()
    }

    pub fn get_liked_songs(&self, limit: usize) -> Vec<Song> {
        let mut stats = self.statistic_repository.get_all();
        stats.sort_by(|a, b| b.liked_count.cmp(&a.liked_count));
        stats
            .into_iter()
            .filter(|stat| !stat.play_item_id.starts_with("radio_uuid_"))
            .filter(|stat| stat.liked_count > 0)
            .filter_map(|stat| self.song_repository.find_by_id(&stat.play_item_id))
            .take(limit)
            .collect()
    }

    pub fn get_library_stats(&self) -> LibraryStats {
        let all_songs: Vec<Song> = self.song_repository.find_all();
        let total_songs = all_songs.len();
        let total_duration_secs = all_songs.iter().filter_map(|s| s.time).map(|d| d.as_secs()).sum();

        let mut genre_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for song in &all_songs {
            if let Some(genre) = &song.genre {
                *genre_map.entry(genre.clone()).or_insert(0) += 1;
            }
        }
        let mut top_genres: Vec<(String, usize)> = genre_map.into_iter().collect();
        top_genres.sort_by(|a, b| b.1.cmp(&a.1));
        top_genres.truncate(10);

        let total_albums = self.album_repository.find_all().len();
        let total_artists = self.album_repository.find_all_album_artists().len();

        let albums_by_decade: Vec<(String, usize)> = self
            .album_repository
            .find_all_by_decade(usize::MAX)
            .into_iter()
            .map(|(decade, albums)| (decade, albums.len()))
            .collect();

        let all_stats = self.statistic_repository.get_all();
        let total_plays = all_stats.iter().map(|s| s.play_count.max(0).cast_unsigned()).sum();
        let unique_songs_played = all_stats
            .iter()
            .filter(|s| s.play_count > 0 && !s.play_item_id.starts_with("radio_uuid_"))
            .count();
        let liked_songs = all_stats
            .iter()
            .filter(|s| s.liked_count > 0 && !s.play_item_id.starts_with("radio_uuid_"))
            .count();

        LibraryStats {
            total_songs,
            total_albums,
            total_artists,
            total_duration_secs,
            total_plays,
            unique_songs_played,
            liked_songs,
            songs_loudness_analysed: 0, // filled by the command handler
            top_genres,
            albums_by_decade,
        }
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
        }
    }

    pub fn search_local_files_by_dir(&self, dir: &str) -> Vec<MetadataLibraryItem> {
        let start_time = std::time::Instant::now();
        let result = self.song_repository.find_by_key_prefix(dir).filter_map(|(key, value)| {
            let key = String::from_utf8(key).ok()?;
            let (_, right) = key.split_once(dir)?;
            if right.contains('/') {
                let (left, _) = right.split_once('/')?;
                Some(MetadataLibraryItem::Directory { name: left.to_owned() })
            } else if let Some(song) = Song::bytes_to_song(&value) {
                Some(MetadataLibraryItem::SongItem(song))
            } else {
                warn!("Failed to deserialize song for key: {key}");
                None
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
            .filter_map(|(key, value)| {
                let key = String::from_utf8(key).ok()?;
                if let Some((path, _)) = key.rsplit_once('/') {
                    if path.to_lowercase().contains(search_term.to_lowercase().as_str()) {
                        Some(MetadataLibraryItem::Directory { name: path.to_owned() })
                    } else if let Some(song) = Song::bytes_to_song(&value) {
                        Some(MetadataLibraryItem::SongItem(song))
                    } else {
                        warn!("Failed to deserialize song for key: {key}");
                        None
                    }
                } else {
                    None
                }
            })
            .take(limit);
        let mut unique: Vec<MetadataLibraryItem> = result.collect();
        unique.dedup();
        log::info!("search_local_files_by_dir_contains took {:?}", start_time.elapsed());
        unique
    }

    pub fn scan_music_dir(&self, full_scan: bool, state_changes_sender: &Sender<StateChangeEvent>) {
        if self.scan_running.load(Ordering::Relaxed) {
            return;
        }
        self.scan_running.store(true, Ordering::Relaxed);
        let settings = self.settings.read().expect("settings lock poisoned").clone();
        let start_time = time::Instant::now();
        if let Err(e) = state_changes_sender.send(StateChangeEvent::MetadataSongScanStarted) {
            warn!("Failed to send scan started event: {e}");
        }
        if full_scan {
            self.song_repository.delete_all();
            self.album_repository.delete_all();
        }

        if !Path::new(ARTWORK_DIR).exists() {
            _ = std::fs::create_dir(ARTWORK_DIR);
        }
        let (new_files, deleted_db_keys) = self.get_diff(&settings);
        info!("Scanning directories: {:?}", settings.effective_directories());
        info!(
            "New files found: {} / Deleted files found: {}",
            new_files.len(),
            deleted_db_keys.len()
        );
        let count = self.add_songs_to_db(&new_files, state_changes_sender, &settings);

        if !full_scan {
            info!("Deleting {} files from database", deleted_db_keys.len());
            for db_key in &deleted_db_keys {
                self.song_repository.delete(db_key);
                if let Err(e) = state_changes_sender.send(StateChangeEvent::MetadataSongScanned(format!(
                    "Key {db_key} deleted from database"
                ))) {
                    warn!("Failed to send scan status event: {e}");
                }
            }
        }
        if let Err(e) = state_changes_sender.send(StateChangeEvent::MetadataSongScanFinished(format!(
            "Music directory scan finished: {count} files scanned in {} seconds",
            start_time.elapsed().as_secs()
        ))) {
            warn!("Failed to send scan finished event: {e}");
        }
        info!(
            "Scanning of {} files finished in {}s",
            count,
            start_time.elapsed().as_secs()
        );
        self.scan_running.store(false, Ordering::Relaxed);
        if let Err(e) = self.db.persist(PersistMode::SyncData) {
            warn!("Failed to persist database after scan: {e}");
        } else {
            info!("Database persisted after scan");
        }
    }

    fn full_path_to_database_key_for_dir(music_dir: &str, input: &str) -> String {
        let mut music_dir_pref = music_dir.to_string();
        if !music_dir_pref.ends_with('/') {
            music_dir_pref.push('/');
        }
        let file_path = input.replace(&music_dir_pref, "");
        to_database_key(file_path.as_str())
    }

    fn full_path_to_database_key(settings: &MetadataStoreSettings, input: &str) -> String {
        for dir in &settings.effective_directories() {
            let mut prefix = dir.clone();
            if !prefix.ends_with('/') {
                prefix.push('/');
            }
            if input.starts_with(&prefix) {
                return Self::full_path_to_database_key_for_dir(dir, input);
            }
        }
        to_database_key(input)
    }

    fn get_diff(&self, settings: &MetadataStoreSettings) -> (Vec<String>, Vec<String>) {
        let mut added_files: Vec<String> = Vec::new();
        let mut deleted_keys: Vec<String> = Vec::new();
        let mut unchanged_keys: Vec<String> = Vec::new();

        let supported_exts = MetadataStoreSettings::default().supported_extensions;
        debug!("Supported extensions: {supported_exts:?}");

        for music_dir in &settings.effective_directories() {
            if !Path::new(music_dir).exists() {
                warn!("Music directory does not exist, skipping: {music_dir}");
                continue;
            }
            for entry in WalkDir::new(music_dir)
                .follow_links(settings.follow_links)
                .sort_by_file_name()
                .into_iter()
                .filter_map(|e| match e {
                    Ok(entry) => Some(entry),
                    Err(err) => {
                        warn!("WalkDir error in {music_dir}: {err}");
                        None
                    }
                })
                .filter(|de| {
                    debug!("Checking file: {}", de.path().display());
                    de.file_type().is_file()
                })
                .filter(|de| {
                    let ext = de.path().extension().map_or_else(
                        || "no_ext".to_string(),
                        |ex| ex.to_str().unwrap_or("no_ext").to_lowercase(),
                    );
                    let is_supported = MetadataStoreSettings::default().supported_extensions.contains(&ext);
                    if !is_supported {
                        debug!("File {} has unsupported extension: {}", de.path().display(), ext);
                    }
                    is_supported
                })
                .filter_map(|de| {
                    let path_str = de.path().to_str()?;
                    Some((
                        path_str.to_owned(),
                        Self::full_path_to_database_key_for_dir(music_dir, path_str),
                    ))
                })
            {
                debug!("Processing file: {} -> key: {}", entry.0, entry.1);
                let is_iso = entry.0.to_lowercase().ends_with(".iso");
                if is_iso {
                    // SACD ISO: each track is stored as a virtual key "{iso_key}#SACD_{idx}".
                    // Treat the ISO as unchanged if any such tracks are already in the database.
                    let sacd_prefix = format!("{}{}", entry.1, SACD_TRACK_MARKER);
                    let existing: Vec<String> = self
                        .song_repository
                        .find_by_key_prefix(&sacd_prefix)
                        .map(|(k, _)| String::from_utf8_lossy(&k).to_string())
                        .collect();
                    if existing.is_empty() {
                        debug!("SACD ISO {} is NEW", entry.0);
                        added_files.push(entry.0.clone());
                    } else {
                        debug!("SACD ISO {} already scanned ({} tracks)", entry.0, existing.len());
                        unchanged_keys.extend(existing);
                    }
                } else if self.song_repository.find_by_id(&entry.1).is_some() {
                    debug!("File {} already in database (unchanged)", entry.0);
                    unchanged_keys.push(entry.1.clone());
                } else {
                    debug!("File {} is NEW", entry.0);
                    added_files.push(entry.0.clone());
                }
            }
        }
        for song in self.song_repository.get_all_iterator() {
            if !unchanged_keys.contains(&song.file) {
                deleted_keys.push(song.file);
            }
        }
        (added_files, deleted_keys)
    }

    fn add_songs_to_db(
        &self,
        files: &[String],
        state_changes_sender: &Sender<StateChangeEvent>,
        settings: &MetadataStoreSettings,
    ) -> u32 {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicU32, Ordering};

        let count = AtomicU32::new(0);
        files.par_iter().for_each(|file| {
            let c = count.fetch_add(1, Ordering::Relaxed);
            state_changes_sender
                .send(StateChangeEvent::MetadataSongScanned(format!("Scanning: {c}. {file}")))
                .ok();
            if let Err(e) = self.scan_single_file(Path::new(file), settings) {
                log::error!("Unable to scan file {file}. Error: {e}");
            }
            if c.is_multiple_of(100) {
                self.song_repository.flush();
            }
        });
        self.song_repository.flush();
        count.load(Ordering::Relaxed)
    }

    /// Fast metadata extraction for APE files using the `ape_decoder` crate directly.
    /// Reads only header + seek table + tags from disk, avoiding loading the entire file.
    fn scan_ape_file_fast(
        &self,
        file_path: &Path,
        settings: &MetadataStoreSettings,
        file_modification_date: DateTime<Utc>,
    ) -> Result<()> {
        let path_str = file_path
            .to_str()
            .ok_or_else(|| Error::msg("APE file path is not valid UTF-8"))?;
        let file_p = Self::full_path_to_database_key(settings, path_str);
        info!("Scanning APE file (fast path):\t{file_p}");

        let file = File::open(file_path)?;
        let mut decoder =
            ape_decoder::ApeDecoder::new(file).map_err(|e| Error::msg(format!("APE decode error: {e}")))?;

        let info = decoder.info();
        let duration_ms = info.duration_ms;
        let mut song = Song {
            time: Some(std::time::Duration::from_millis(duration_ms)),
            ..Default::default()
        };

        // Read APEv2 tags
        if let Ok(Some(ape_tag)) = decoder.read_tag() {
            for field in &ape_tag.fields {
                if let Some(value) = field.value_as_str() {
                    let name = field.name.to_ascii_lowercase();
                    match name.as_str() {
                        "title" => song.title = Some(value.to_string()),
                        "artist" => song.artist = Some(value.to_string()),
                        "album" => song.album = Some(value.to_string()),
                        "album artist" | "albumartist" => song.album_artist = Some(value.to_string()),
                        "year" | "date" => song.date = Some(value.to_string()),
                        "track" | "tracknumber" => song.track = Some(value.to_string()),
                        "disc" | "discnumber" => song.disc = Some(value.to_string()),
                        "genre" => song.genre = Some(value.to_string()),
                        "composer" => song.composer = Some(value.to_string()),
                        "performer" => song.performer = Some(value.to_string()),
                        "label" => song.label = Some(value.to_string()),
                        _ => {
                            song.tags.entry(field.name.clone()).or_insert_with(|| value.to_string());
                        }
                    }
                }
            }
        }

        // Read ID3v2 tags as fallback
        if let Ok(Some(id3_tag)) = decoder.read_id3v2_tag() {
            for frame in &id3_tag.frames {
                if let Some(value) = Self::decode_id3_text_frame(&frame.data) {
                    match frame.id.as_str() {
                        "TIT2" if song.title.is_none() => song.title = Some(value),
                        "TPE1" if song.artist.is_none() => song.artist = Some(value),
                        "TALB" if song.album.is_none() => song.album = Some(value),
                        "TPE2" if song.album_artist.is_none() => song.album_artist = Some(value),
                        "TDRC" | "TYER" | "TDAT" if song.date.is_none() => song.date = Some(value),
                        "TRCK" if song.track.is_none() => {
                            song.track = Some(value.split('/').next().unwrap_or(&value).to_string());
                        }
                        "TPOS" if song.disc.is_none() => {
                            song.disc = Some(value.split('/').next().unwrap_or(&value).to_string());
                        }
                        "TCON" if song.genre.is_none() => song.genre = Some(value),
                        "TCOM" if song.composer.is_none() => song.composer = Some(value),
                        "TPE3" if song.performer.is_none() => song.performer = Some(value),
                        "TPUB" if song.label.is_none() => song.label = Some(value),
                        _ => {}
                    }
                }
            }
        }

        song.file.clone_from(&file_p);
        song.file_date = file_modification_date;
        debug!("Add/update song in database: {song:?}");
        self.song_repository.save(&song);
        self.album_repository.update_from_song(song);
        Ok(())
    }

    /// Decode an `ID3v2` text frame payload to a string.
    fn decode_id3_text_frame(data: &[u8]) -> Option<String> {
        if data.is_empty() {
            return None;
        }
        let encoding = data[0];
        let payload = &data[1..];
        if payload.is_empty() {
            return None;
        }
        let text = match encoding {
            0 => payload.iter().map(|&b| b as char).collect::<String>(),
            1..=3 => String::from_utf8_lossy(payload).into_owned(),
            _ => return None,
        };
        let text = text.trim_end_matches('\0').to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// Scan an SACD ISO file and create one `Song` entry per audio track using virtual paths.
    fn scan_sacd_iso_file(
        &self,
        file_path: &Path,
        settings: &MetadataStoreSettings,
        file_modification_date: DateTime<Utc>,
    ) -> Result<()> {
        let path_str = file_path
            .to_str()
            .ok_or_else(|| Error::msg("SACD ISO path is not valid UTF-8"))?;
        let iso_key = Self::full_path_to_database_key(settings, path_str);
        info!("Scanning SACD ISO:\t{iso_key}");

        let mut file = File::open(file_path).map_err(|e| Error::msg(format!("Cannot open SACD ISO: {e}")))?;

        let mode = detect_sector_mode(&mut file).map_err(|e| Error::msg(format!("Not a valid SACD ISO ({e})")))?;
        let areas = read_areas(&mut file, mode).map_err(|e| Error::msg(format!("Failed to read SACD areas: {e}")))?;

        // Prefer stereo area; fall back to first.
        let area = areas
            .iter()
            .find(|a| a.is_stereo)
            .or_else(|| areas.first())
            .ok_or_else(|| Error::msg("No playable SACD area found"))?;

        let tracks = read_tracks(&mut file, mode, area)
            .map_err(|e| Error::msg(format!("Failed to read SACD track list: {e}")))?;

        if tracks.is_empty() {
            return Err(Error::msg("SACD ISO contains no tracks"));
        }

        for (idx, track) in tracks.iter().enumerate() {
            let virtual_key = format!("{iso_key}{SACD_TRACK_MARKER}{idx:04}");
            let duration_secs = track.duration_secs(area.channel_count, area.frame_format);

            let song = Song {
                title: Some(format!("Track {}", idx + 1)),
                track: Some(format!("{}", idx + 1)),
                time: Some(std::time::Duration::from_secs_f64(duration_secs)),
                file: virtual_key.clone(),
                file_date: file_modification_date,
                ..Default::default()
            };

            log::debug!("SACD track {idx}: {virtual_key} ({duration_secs:.1}s)");
            self.song_repository.save(&song);
            self.album_repository.update_from_song(song);
        }

        Ok(())
    }

    fn scan_single_file(&self, file_path: &Path, settings: &MetadataStoreSettings) -> Result<()> {
        info!("Scanning file:\t{}", file_path.display());

        // Fast path for APE files: read tags directly without loading entire file.
        if file_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ape")) {
            let file_modification_date: DateTime<Utc> = file_path.metadata()?.modified()?.into();
            return self.scan_ape_file_fast(file_path, settings, file_modification_date);
        }

        // SACD ISO: expand to one Song entry per audio track.
        if file_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")) {
            let file_modification_date: DateTime<Utc> = file_path.metadata()?.modified()?.into();
            return self.scan_sacd_iso_file(file_path, settings, file_modification_date);
        }

        let file = Box::new(File::open(file_path)?);
        let file_modification_date: DateTime<Utc> = file.as_ref().metadata()?.modified()?.into();

        let mss = MediaSourceStream::new(file, symphonia::core::io::MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = file_path.extension() {
            if let Some(ext_str) = ext.to_str() {
                hint.with_extension(&ext_str.to_lowercase());
            }
        }
        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let path_str = file_path
            .to_str()
            .ok_or_else(|| Error::msg("File path is not valid UTF-8"))?;
        let file_p = &Self::full_path_to_database_key(settings, path_str);

        info!("Scanning file:\t{file_p}");
        match crate::build_probe().probe(&hint, mss, format_opts, metadata_opts) {
            Ok(mut probed) => {
                let (mut song, image_data) = AudioMetadataExtractor::extract(&mut *probed);

                if let Some(image_data) = &image_data {
                    let image_id = uuid::Uuid::new_v4();
                    if let Err(e) = std::fs::write(Path::new(ARTWORK_DIR).join(image_id.to_string()), &image_data.data)
                    {
                        warn!("Error writing image file: {e}");
                    } else {
                        song.image_id = Some(image_id.to_string());
                    }
                }

                song.file.clone_from(file_p);
                song.file_date = file_modification_date;
                log::debug!("Add/update song in database: {song:?}");
                self.song_repository.save(&song);
                self.album_repository.update_from_song(song);

                Ok(())
            }
            Err(err) => {
                warn!("Error:{file_p} {err}");
                Err(Error::msg(format!("Error:{file_p} {err}")))
            }
        }
    }
}
