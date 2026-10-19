//! Library scanner and metadata queries.
//!
//! [`MetadataService`] walks the configured music directories, probes each
//! file with Symphonia (plus the custom APE/DSF/SACD readers), extracts tags
//! and artwork (written to `artwork/<uuid>`, referenced by `Song.image_id`)
//! and fills the
//! song/album repositories. Scans run on a background thread guarded by
//! `scan_running`; progress streams to clients as `MetadataSongScan*` events.
//! SACD ISOs expand into one virtual song per track (`#SACD_` marker in the
//! file key). Also answers browse/search queries and computes
//! [`api_models::stat::LibraryStats`].

use std::{
    cell::Cell,
    cmp::Reverse,
    collections::HashSet,
    fs::File,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time,
};

use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use log::{debug, info, warn};
use symphonia::core::{
    formats::{FormatOptions, probe::Hint},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
};
use tokio::sync::broadcast::Sender;
use walkdir::WalkDir;

use api_models::{
    common::MetadataLibraryItem,
    player::Song,
    radio::RadioStation,
    settings::MetadataStoreSettings,
    stat::{LibraryStats, PlayItemStatistics},
    state::{ScanPhase, ScanProgress, ScanSummary, StateChangeEvent},
};

use crate::audio_metadata_extractor::AudioMetadataExtractor;
use crate::ports::{
    album_repository::ArcAlbumRepository, play_statistics_repository::ArcPlayStatisticsRepository, song_repository::ArcSongRepository,
};
use crate::outage_gate::{Check, OutageGate};
use crate::remote_store::{self, REMOTE_KEY_PREFIX};
use crate::sacd_bundle::{SACD_TRACK_MARKER, detect_sector_mode, read_areas, read_tracks};

const ARTWORK_DIR: &str = "artwork";

/// Minimum time between two progress events of a running scan.
const PROGRESS_INTERVAL: time::Duration = time::Duration::from_millis(500);

/// Publishes scan progress: keeps the latest state for late clients
/// (`MetadataCommand::QueryScanStatus`) and sends it at most every
/// [`PROGRESS_INTERVAL`], plus whenever the phase changes.
struct ScanReporter<'a> {
    sender: &'a Sender<StateChangeEvent>,
    latest: &'a Mutex<Option<ScanProgress>>,
    last_sent: Cell<Option<time::Instant>>,
    last_phase: Cell<Option<ScanPhase>>,
}

impl<'a> ScanReporter<'a> {
    const fn new(sender: &'a Sender<StateChangeEvent>, latest: &'a Mutex<Option<ScanProgress>>) -> Self {
        Self {
            sender,
            latest,
            last_sent: Cell::new(None),
            last_phase: Cell::new(None),
        }
    }

    fn report(&self, phase: ScanPhase, done: usize, total: usize, current: &str) {
        let due = self.last_phase.get() != Some(phase) || self.last_sent.get().is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL);
        if !due {
            return;
        }
        let progress = ScanProgress {
            phase,
            done: u32::try_from(done).unwrap_or(u32::MAX),
            total: u32::try_from(total).unwrap_or(u32::MAX),
            current: current.to_string(),
        };
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some(progress.clone());
        }
        self.last_sent.set(Some(time::Instant::now()));
        self.last_phase.set(Some(phase));
        self.sender.send(StateChangeEvent::MetadataScanProgress(progress)).ok();
    }
}

/// Most local files read in parallel (capped by the CPU count): local
/// reads are mostly disk bound, and a single-core Pi stays sequential.
const LOCAL_SCAN_WORKERS: usize = 4;

/// A file whose tags a scan reads.
enum ScanJob {
    /// Full path of a local file.
    Local(String),
    /// Database key of a remote object, with its modification time and size.
    Remote { key: String, modified: DateTime<Utc>, size: u64 },
}

impl ScanJob {
    fn label(&self) -> &str {
        match self {
            Self::Local(path) => path,
            Self::Remote { key, .. } => key,
        }
    }
}

/// Why a file of a scan was not read.
enum ReadError {
    /// The file itself failed (bad data, unsupported format…).
    Unreadable(Error),
    /// Its remote storage stayed unreachable.
    Unreached,
}

/// How long a scan waits for an unreachable remote library (counted in
/// pauses between checks) before leaving its remaining files for the next
/// scan.
const MAX_OUTAGE: time::Duration = time::Duration::from_mins(5);
/// Reads of a remote file while its storage answers; failures during an
/// outage don't count.
const REMOTE_READ_ATTEMPTS: u32 = 3;

/// Keys of the `scan_state` keyspace.
const SCAN_RUNNING_KEY: &[u8] = b"running";
const LAST_SUMMARY_KEY: &[u8] = b"last_summary";

/// Last two path segments ("Album/01 Track.flac"), enough to recognise the file.
fn short_file_name(path: &str) -> String {
    let mut parts = path.rsplit('/').filter(|p| !p.is_empty());
    match (parts.next(), parts.next()) {
        (Some(file), Some(dir)) => format!("{dir}/{file}"),
        (Some(file), None) => file.to_string(),
        _ => path.to_string(),
    }
}

/// `75` -> "75 s", `130` -> "2 min 10 s".
fn format_duration(d: time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 { format!("{secs} s") } else { format!("{} min {} s", secs / 60, secs % 60) }
}

/// Result of comparing the libraries with the database.
struct LibraryDiff {
    /// Full paths of new local files.
    added_files: Vec<String>,
    /// Database keys of new remote objects, with modification time and size.
    added_remote: Vec<(String, DateTime<Utc>, u64)>,
    deleted_keys: Vec<String>,
    /// Remote libraries whose listing failed (their songs are kept).
    unavailable: Vec<String>,
}

pub struct MetadataService {
    settings: RwLock<MetadataStoreSettings>,
    scan_running: AtomicBool,
    /// Latest progress of the running scan, `None` when idle.
    scan_progress: Mutex<Option<ScanProgress>>,
    song_repository: ArcSongRepository,
    album_repository: ArcAlbumRepository,
    statistic_repository: ArcPlayStatisticsRepository,
    db: Arc<Database>,
    /// `station_id` -> [`RadioStation`] JSON, for stations entered by hand.
    radio_stations: Keyspace,
    /// A marker while a scan runs, and the last scan's [`ScanSummary`].
    scan_state: Keyspace,
}

impl MetadataService {
    const MIGRATION_MARKER: &'static [u8] = b"cleanup_orphaned_albums_v3_5_5";

    pub fn new(
        db: Arc<Database>,
        settings: &MetadataStoreSettings,
        song_repository: ArcSongRepository,
        album_repository: ArcAlbumRepository,
        statistic_repository: ArcPlayStatisticsRepository,
    ) -> Result<Arc<Self>> {
        let settings = settings.clone();

        Self::run_migration_if_needed(&db, &song_repository, &album_repository);

        let radio_stations = db
            .keyspace("radio_stations", KeyspaceCreateOptions::default)
            .expect("Failed to open radio_stations keyspace");
        let scan_state = db
            .keyspace("scan_state", KeyspaceCreateOptions::default)
            .expect("Failed to open scan_state keyspace");
        Self::note_interrupted_scan(&scan_state);

        Ok(Arc::new(Self {
            settings: RwLock::new(settings),
            scan_running: AtomicBool::new(false),
            scan_progress: Mutex::new(None),
            song_repository,
            album_repository,
            statistic_repository,
            db,
            radio_stations,
            scan_state,
        }))
    }

    /// A scan marker left from the last run means the process ended
    /// mid-scan (closed, killed, or frozen and killed in the background on
    /// Android): files not read by then are missing until the next scan.
    fn note_interrupted_scan(scan_state: &Keyspace) {
        if scan_state.get(SCAN_RUNNING_KEY).ok().flatten().is_none() {
            return;
        }
        warn!("The last library scan did not finish");
        Self::save_summary(
            scan_state,
            &ScanSummary {
                text: "Interrupted before it finished".to_string(),
                complete: false,
            },
        );
        _ = scan_state.remove(SCAN_RUNNING_KEY);
    }

    fn save_summary(scan_state: &Keyspace, summary: &ScanSummary) {
        match serde_json::to_vec(summary) {
            Ok(json) => {
                if let Err(e) = scan_state.insert(LAST_SUMMARY_KEY, json) {
                    warn!("Failed to save scan summary: {e}");
                }
            }
            Err(e) => warn!("Failed to serialize scan summary: {e}"),
        }
    }

    /// Outcome of the last scan, also of one before a restart.
    pub fn last_scan_summary(&self) -> Option<ScanSummary> {
        let bytes = self.scan_state.get(LAST_SUMMARY_KEY).ok().flatten()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn run_migration_if_needed(db: &Database, song_repository: &ArcSongRepository, album_repository: &ArcAlbumRepository) {
        let migrations = db
            .keyspace("_migrations", KeyspaceCreateOptions::default)
            .expect("Failed to open _migrations keyspace");

        if migrations.get(Self::MIGRATION_MARKER).ok().flatten().is_some() {
            return;
        }

        info!("Running one-shot migration: cleanup orphaned albums (pre-3.5.5)");
        let valid_keys: HashSet<String> = song_repository.find_all().into_iter().map(|s| s.file).collect();
        if let Err(e) = album_repository.cleanup_orphaned_albums(&valid_keys) {
            warn!("Migration: failed to cleanup orphaned albums: {e}");
            return;
        }

        if let Err(e) = migrations.insert(Self::MIGRATION_MARKER, b"1") {
            warn!("Migration: failed to store migration marker: {e}");
        } else if let Err(e) = db.persist(PersistMode::SyncData) {
            warn!("Migration: failed to persist after migration: {e}");
        } else {
            info!("Migration: cleanup orphaned albums completed");
        }
    }

    pub fn update_settings(&self, settings: MetadataStoreSettings) {
        *self.settings.write().expect("settings lock poisoned") = settings;
    }

    pub fn settings(&self) -> MetadataStoreSettings {
        self.settings.read().expect("settings lock poisoned").clone()
    }

    pub fn effective_directories(&self) -> Vec<String> {
        self.settings.read().expect("settings lock poisoned").effective_directories()
    }

    pub fn get_favorite_radio_stations(&self) -> Vec<String> {
        self.statistic_repository
            .find_by_key_prefix("radio_uuid_")
            .iter()
            .filter(|stat| stat.liked_count > 0)
            .map(|stat| stat.play_item_id.strip_prefix("radio_uuid_").unwrap_or_default().to_string())
            .collect()
    }

    /// Stations the listener typed in by hand, sorted by name.
    pub fn get_custom_radio_stations(&self) -> Vec<RadioStation> {
        self.stored_radio_stations(false)
    }

    /// Liked radio-browser stations whose details were sent with
    /// `LikeRadioStation`, sorted by name.
    pub fn get_favorite_radio_station_records(&self) -> Vec<RadioStation> {
        let liked = self.get_favorite_radio_stations();
        self.stored_radio_stations(true)
            .into_iter()
            .filter(|s| s.radio_browser_uuid.as_ref().is_some_and(|u| liked.contains(u)))
            .collect()
    }

    /// Stations Next/Prev cycle through while radio plays: favorites, then
    /// hand-added ones, each sorted by name; a stream URL appears once.
    pub fn get_zap_stations(&self) -> Vec<RadioStation> {
        let mut stations = self.get_favorite_radio_station_records();
        for custom in self.get_custom_radio_stations() {
            if !stations.iter().any(|s| s.url == custom.url) {
                stations.push(custom);
            }
        }
        stations
    }

    /// Likes a radio-browser station and stores its details. The like-stat
    /// (`radio_uuid_{uuid}`) stays the source of truth for "is favorite".
    ///
    /// # Errors
    /// When the station has no `radio_browser_uuid`, is invalid, or the write fails.
    pub fn like_radio_station(&self, station: &RadioStation) -> Result<RadioStation> {
        let mut station = station.validated().map_err(Error::msg)?;
        let uuid = station
            .radio_browser_uuid
            .clone()
            .filter(|u| !u.is_empty())
            .ok_or_else(|| Error::msg("Station has no radio-browser uuid"))?;
        let key = favorite_station_key(&uuid);
        let already_liked = self.get_favorite_radio_stations().contains(&uuid);
        if !already_liked {
            self.like_media_item(&format!("radio_uuid_{uuid}"));
        }
        station.id.clone_from(&key);
        if station.added_at.is_none() {
            station.added_at = Some(Utc::now());
        }
        self.radio_stations.insert(&key, serde_json::to_vec(&station)?)?;
        self.db.persist(PersistMode::SyncData)?;
        Ok(station)
    }

    fn stored_radio_stations(&self, favorites: bool) -> Vec<RadioStation> {
        let mut stations: Vec<RadioStation> = self
            .radio_stations
            .iter()
            .filter_map(|guard| {
                let value = guard.value().ok()?;
                serde_json::from_slice::<RadioStation>(&value)
                    .map_err(|e| warn!("Failed to decode stored radio station: {e}"))
                    .ok()
            })
            .filter(|s| s.radio_browser_uuid.is_some() == favorites)
            .collect();
        stations.sort_by_key(|a| a.name.to_lowercase());
        stations
    }

    /// Stores a hand-added station, assigning an id and `added_at` when the
    /// incoming `id` is empty. Returns the stored station.
    ///
    /// # Errors
    /// When the station is invalid (see [`RadioStation::validated`]), when it
    /// duplicates the stream URL of another station, or when the write fails.
    pub fn save_custom_radio_station(&self, station: &RadioStation) -> Result<RadioStation> {
        let mut station = station.validated().map_err(Error::msg)?;
        station.radio_browser_uuid = None;
        let existing = self.get_custom_radio_stations();
        if let Some(dup) = existing.iter().find(|s| s.url == station.url && s.id != station.id) {
            return Err(Error::msg(format!("Stream URL is already saved as '{}'", dup.name)));
        }
        if station.id.is_empty() {
            station.id = uuid::Uuid::new_v4().to_string();
            station.added_at = Some(Utc::now());
        } else if station.added_at.is_none() {
            station.added_at = existing.iter().find(|s| s.id == station.id).and_then(|s| s.added_at);
        }
        let json = serde_json::to_vec(&station)?;
        self.radio_stations.insert(&station.id, json)?;
        self.db.persist(PersistMode::SyncData)?;
        Ok(station)
    }

    /// # Errors
    /// When the delete or the following flush fails.
    pub fn delete_custom_radio_station(&self, id: &str) -> Result<()> {
        self.radio_stations.remove(id)?;
        self.db.persist(PersistMode::SyncData)?;
        Ok(())
    }

    pub fn get_most_played_songs(&self, limit: usize) -> Vec<Song> {
        let mut stats = self.statistic_repository.get_all();
        stats.sort_by_key(|b| Reverse(b.play_count));
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
        stats.sort_by_key(|b| Reverse(b.liked_count));
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
        top_genres.sort_by_key(|b| Reverse(b.1));
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
        if let Some(uuid) = media_item_id.strip_prefix("radio_uuid_")
            && !self.get_favorite_radio_stations().iter().any(|u| u == uuid)
        {
            _ = self.radio_stations.remove(favorite_station_key(uuid));
        }
    }

    pub fn increase_play_count(&self, media_item_id: &str) {
        self.update_or_create_media_item_stat(media_item_id, |item| item.play_count += 1);
    }

    fn update_or_create_media_item_stat<J>(&self, media_item_id: &str, mut job: J)
    where
        J: FnMut(&mut PlayItemStatistics),
    {
        let stat = if let Some(mut stat) = self.statistic_repository.find_by_id(media_item_id) {
            job(&mut stat);
            stat
        } else {
            let mut stat = PlayItemStatistics {
                play_item_id: media_item_id.to_string(),
                ..Default::default()
            };
            job(&mut stat);
            stat
        };
        if let Err(e) = self.statistic_repository.save(&stat) {
            warn!("Failed to save stats for '{media_item_id}': {e}");
        }
    }

    pub fn search_local_files_by_dir(&self, dir: &str) -> Vec<MetadataLibraryItem> {
        let start_time = std::time::Instant::now();
        let result = self.song_repository.find_by_key_prefix(dir).into_iter().filter_map(|(key, value)| {
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
            .into_iter()
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

    /// Progress of the running scan, `None` when no scan runs.
    pub fn scan_progress(&self) -> Option<ScanProgress> {
        self.scan_progress.lock().ok().and_then(|p| p.clone())
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
        _ = self.scan_state.insert(SCAN_RUNNING_KEY, b"1");
        // Durable now: a scan killed early must still be noticed.
        if let Err(e) = self.db.persist(PersistMode::SyncData) {
            warn!("Failed to persist scan marker: {e}");
        }
        let reporter = ScanReporter::new(state_changes_sender, &self.scan_progress);
        reporter.report(ScanPhase::Listing, 0, 0, "");
        if full_scan {
            self.song_repository.delete_all();
            self.album_repository.delete_all();
        }

        if !Path::new(ARTWORK_DIR).exists() {
            _ = std::fs::create_dir(ARTWORK_DIR);
        }
        let LibraryDiff {
            added_files,
            added_remote,
            deleted_keys: deleted_db_keys,
            unavailable,
        } = self.get_diff(&settings, &reporter);
        info!("Scanning directories: {:?}", settings.effective_directories());
        info!(
            "New files found: {} local, {} remote / Deleted files found: {}",
            added_files.len(),
            added_remote.len(),
            deleted_db_keys.len()
        );
        let (count, failed, unreached) = self.add_songs_to_db(&added_files, &added_remote, &reporter, &settings);

        let removed = if full_scan { 0 } else { deleted_db_keys.len() };
        if !full_scan {
            info!("Deleting {} files from database", deleted_db_keys.len());
            for (i, db_key) in deleted_db_keys.iter().enumerate() {
                reporter.report(ScanPhase::Removing, i, deleted_db_keys.len(), db_key);
                if let Some(song) = self.song_repository.find_by_id(db_key)
                    && let Err(e) = self.album_repository.remove_from_song(&song)
                {
                    warn!("Failed to remove song '{db_key}' from album: {e}");
                }
                if let Err(e) = self.song_repository.delete(db_key) {
                    warn!("Failed to delete song '{db_key}' from db: {e}");
                }
            }
        }
        let added = count - failed - unreached;
        let mut problems = Vec::new();
        if failed > 0 {
            problems.push(format!("{failed} unreadable"));
        }
        if unreached > 0 {
            problems.push(format!("{unreached} not read (storage unreachable)"));
        }
        problems.extend(unavailable.iter().map(|name| format!("{name} could not be reached")));
        let problems = if problems.is_empty() { String::new() } else { format!(", {}", problems.join(", ")) };
        let summary = if added == 0 && removed == 0 && problems.is_empty() {
            format!("Library is up to date ({})", format_duration(start_time.elapsed()))
        } else {
            format!(
                "Library updated: {added} new, {removed} removed{problems} ({})",
                format_duration(start_time.elapsed())
            )
        };
        let scan_summary = ScanSummary {
            text: summary.clone(),
            complete: problems.is_empty(),
        };
        Self::save_summary(&self.scan_state, &scan_summary);
        _ = self.scan_state.remove(SCAN_RUNNING_KEY);
        state_changes_sender.send(StateChangeEvent::MetadataScanSummary(scan_summary)).ok();
        if let Ok(mut latest) = self.scan_progress.lock() {
            *latest = None;
        }
        if let Err(e) = state_changes_sender.send(StateChangeEvent::MetadataSongScanFinished(summary.clone())) {
            warn!("Failed to send scan finished event: {e}");
        }
        info!("Scan finished: {summary}");
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
        input.replace(&music_dir_pref, "")
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
        input.to_string()
    }

    fn get_diff(&self, settings: &MetadataStoreSettings, reporter: &ScanReporter) -> LibraryDiff {
        let mut added_files: Vec<String> = Vec::new();
        let mut deleted_keys: Vec<String> = Vec::new();
        let mut unchanged_keys: HashSet<String> = HashSet::new();

        let supported_exts = MetadataStoreSettings::default().supported_extensions;
        debug!("Supported extensions: {supported_exts:?}");

        let mut checked = 0usize;
        for music_dir in &settings.effective_directories() {
            reporter.report(ScanPhase::Listing, checked, 0, music_dir);
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
                    let is_supported = is_supported_file(de.path(), &supported_exts);
                    if !is_supported {
                        debug!("File {} has unsupported extension", de.path().display());
                    }
                    is_supported
                })
                .filter_map(|de| {
                    let path_str = de.path().to_str()?;
                    Some((path_str.to_owned(), Self::full_path_to_database_key_for_dir(music_dir, path_str)))
                })
            {
                debug!("Processing file: {} -> key: {}", entry.0, entry.1);
                checked += 1;
                reporter.report(ScanPhase::Listing, checked, 0, music_dir);
                let is_iso = entry.0.to_lowercase().ends_with(".iso");
                if is_iso {
                    // SACD ISO: each track is stored as a virtual key "{iso_key}#SACD_{idx}".
                    // Treat the ISO as unchanged if any such tracks are already in the database.
                    let sacd_prefix = format!("{}{}", entry.1, SACD_TRACK_MARKER);
                    let existing: Vec<String> = self
                        .song_repository
                        .find_by_key_prefix(&sacd_prefix)
                        .into_iter()
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
                    unchanged_keys.insert(entry.1.clone());
                } else {
                    debug!("File {} is NEW", entry.0);
                    added_files.push(entry.0.clone());
                }
            }
        }
        let mut unavailable = Vec::new();
        let added_remote = self.remote_diff(settings, &supported_exts, &mut unchanged_keys, &mut unavailable, reporter, checked);
        for song in self.song_repository.find_all() {
            if !unchanged_keys.contains(&song.file) {
                deleted_keys.push(song.file);
            }
        }
        LibraryDiff {
            added_files,
            added_remote,
            deleted_keys,
            unavailable,
        }
    }

    /// Lists every enabled remote library; returns the keys of new objects
    /// and records existing ones in `unchanged_keys`. When a listing fails
    /// (server offline, VPS down) the library's songs are kept as they are
    /// instead of being treated as deleted.
    fn remote_diff(
        &self,
        settings: &MetadataStoreSettings,
        supported_exts: &[String],
        unchanged_keys: &mut HashSet<String>,
        unavailable: &mut Vec<String>,
        reporter: &ScanReporter,
        mut checked: usize,
    ) -> Vec<(String, DateTime<Utc>, u64)> {
        let mut added = Vec::new();
        for library in settings.remote_libraries.iter().filter(|l| l.enabled) {
            reporter.report(ScanPhase::Listing, checked, 0, &library.name);
            let listing = remote_store::store_for(library).and_then(|store| store.list());
            let objects = match listing {
                Ok(objects) => objects,
                Err(e) => {
                    warn!("Remote library '{}' unavailable, keeping its songs: {e:#}", library.name);
                    unavailable.push(library.name.clone());
                    let prefix = format!("{REMOTE_KEY_PREFIX}{}/", library.id);
                    unchanged_keys.extend(
                        self.song_repository
                            .find_by_key_prefix(&prefix)
                            .into_iter()
                            .map(|(k, _)| String::from_utf8_lossy(&k).to_string()),
                    );
                    continue;
                }
            };
            info!("Remote library '{}': {} objects listed", library.name, objects.len());
            checked += objects.len();
            reporter.report(ScanPhase::Listing, checked, 0, &library.name);
            for object in objects {
                let path = Path::new(&object.path);
                // APE fast path and SACD ISO need a local file.
                let local_only = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("ape") || e.eq_ignore_ascii_case("iso"));
                if local_only || !is_supported_file(path, supported_exts) {
                    continue;
                }
                let key = remote_store::remote_key(&library.id, &object.path);
                if self.song_repository.find_by_id(&key).is_some() {
                    unchanged_keys.insert(key);
                } else {
                    added.push((key, object.last_modified.unwrap_or_else(Utc::now), object.size));
                }
            }
        }
        added
    }

    /// Reads tags of the new files; returns (files handled, files that
    /// failed, remote files not read because their storage stayed
    /// unreachable).
    ///
    /// Workers read files in parallel (open, tags, artwork); every song is
    /// saved here on the scanner thread, so album updates never race. APE and
    /// SACD ISO files (local only) keep their special readers, one at a time.
    fn add_songs_to_db(
        &self,
        files: &[String],
        remote: &[(String, DateTime<Utc>, u64)],
        reporter: &ScanReporter,
        settings: &MetadataStoreSettings,
    ) -> (usize, usize, usize) {
        let total = files.len() + remote.len();
        let mut done = 0;
        let mut failed = 0;
        let mut unreached = 0;

        let (special, plain): (Vec<&String>, Vec<&String>) = files.iter().partition(|f| {
            Path::new(f.as_str())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ape") || e.eq_ignore_ascii_case("iso"))
        });
        for file in special {
            reporter.report(ScanPhase::Reading, done, total, &short_file_name(file));
            if let Err(e) = self.scan_single_file(Path::new(file), settings) {
                log::error!("Unable to scan file {file}. Error: {e}");
                failed += 1;
            }
            done += 1;
        }

        let jobs: Vec<ScanJob> = plain
            .into_iter()
            .map(|f| ScanJob::Local(f.clone()))
            .chain(remote.iter().map(|(key, modified, size)| ScanJob::Remote {
                key: key.clone(),
                modified: *modified,
                size: *size,
            }))
            .collect();
        let workers = if remote.is_empty() {
            std::thread::available_parallelism().map_or(1, usize::from).min(LOCAL_SCAN_WORKERS)
        } else {
            remote_store::SCAN_WORKERS
        }
        .min(jobs.len());
        info!("Reading {} files with {workers} workers", jobs.len());

        let next = AtomicUsize::new(0);
        let gate = OutageGate::new(MAX_OUTAGE);
        let (tx, rx) = std::sync::mpsc::channel::<(usize, std::result::Result<Song, ReadError>)>();
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let tx = tx.clone();
                let (next, jobs, gate) = (&next, &jobs, &gate);
                scope.spawn(move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        let result = match job {
                            ScanJob::Local(_) => Self::read_song(job, settings).map_err(ReadError::Unreadable),
                            ScanJob::Remote { key, .. } => Self::read_remote_song(job, key, settings, gate),
                        };
                        if tx.send((i, result)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            loop {
                let (i, result) = match rx.recv_timeout(time::Duration::from_secs(1)) {
                    Ok(received) => received,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if gate.is_down() {
                            reporter.report(ScanPhase::Reading, done, total, "Connection lost, waiting for it to come back…");
                        }
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let label = jobs[i].label();
                match result {
                    Ok(song) => {
                        if let Err(e) = self.save_song(song) {
                            log::error!("Unable to save song {label}. Error: {e}");
                            failed += 1;
                        }
                    }
                    Err(ReadError::Unreadable(e)) => {
                        log::error!("Unable to scan file {label}. Error: {e}");
                        failed += 1;
                    }
                    Err(ReadError::Unreached) => unreached += 1,
                }
                done += 1;
                reporter.report(ScanPhase::Reading, done, total, &short_file_name(label));
                if done.is_multiple_of(100) {
                    self.song_repository.flush();
                }
            }
        });
        self.song_repository.flush();
        if unreached > 0 {
            warn!("{unreached} remote files not read: storage unreachable for over {} min", MAX_OUTAGE.as_secs() / 60);
        }
        (total, failed, unreached)
    }

    /// Reads a remote file, waiting out network outages together with the
    /// other scan workers (see [`OutageGate`]). Only failures while the
    /// storage answers count against the file.
    fn read_remote_song(job: &ScanJob, key: &str, settings: &MetadataStoreSettings, gate: &OutageGate) -> std::result::Result<Song, ReadError> {
        // Bounds retries if the connection keeps flapping.
        const MAX_READS: u32 = 10;
        let mut attempts = 0;
        for _ in 0..MAX_READS {
            if !gate.wait() {
                return Err(ReadError::Unreached);
            }
            let err = match Self::read_song(job, settings) {
                Ok(song) => return Ok(song),
                Err(e) => e,
            };
            match gate.after_failure(|| remote_store::is_reachable(settings, key), std::thread::sleep) {
                Check::Unreachable => return Err(ReadError::Unreached),
                Check::CameBack => warn!("Reading {key} failed during an outage, retrying: {err:#}"),
                Check::Reachable => {
                    attempts += 1;
                    if attempts >= REMOTE_READ_ATTEMPTS {
                        return Err(ReadError::Unreadable(err));
                    }
                    warn!("Reading {key} failed ({attempts}/{REMOTE_READ_ATTEMPTS}), retrying: {err:#}");
                }
            }
        }
        Err(ReadError::Unreached)
    }

    /// Reads one file's song (tags and artwork); no database access, so it
    /// runs on scan workers. A panic in a decoder fails just this file.
    fn read_song(job: &ScanJob, settings: &MetadataStoreSettings) -> Result<Song> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match job {
            ScanJob::Local(path) => {
                let file_path = Path::new(path);
                info!("Scanning file:\t{path}");
                let file = File::open(file_path)?;
                let modified: DateTime<Utc> = file.metadata()?.modified()?.into();
                let key = Self::full_path_to_database_key(settings, path);
                let ext = file_path.extension().and_then(|e| e.to_str());
                Self::probe_song(Box::new(file), ext, &key, modified)
            }
            ScanJob::Remote { key, modified, size } => {
                info!("Scanning remote file:\t{key}");
                let source = remote_store::open_for_scan(settings, key, *size)?;
                let ext = Path::new(key).extension().and_then(|e| e.to_str());
                Self::probe_song(Box::new(source), ext, key, *modified)
            }
        }))
        .map_err(|_| Error::msg(format!("panicked while reading {}", job.label())))?
    }

    fn save_song(&self, song: Song) -> Result<()> {
        log::debug!("Add/update song in database: {song:?}");
        self.song_repository.save(&song)?;
        self.album_repository.update_from_song(song)?;
        Ok(())
    }


    /// Fast metadata extraction for APE files using the `ape_decoder` crate directly.
    /// Reads only header + seek table + tags from disk, avoiding loading the entire file.
    fn scan_ape_file_fast(&self, file_path: &Path, settings: &MetadataStoreSettings, file_modification_date: DateTime<Utc>) -> Result<()> {
        let path_str = file_path.to_str().ok_or_else(|| Error::msg("APE file path is not valid UTF-8"))?;
        let file_p = Self::full_path_to_database_key(settings, path_str);
        info!("Scanning APE file (fast path):\t{file_p}");

        let file = File::open(file_path)?;
        let mut decoder = ape_decoder::ApeDecoder::new(file).map_err(|e| Error::msg(format!("APE decode error: {e}")))?;

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
                    let trimmed = value.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let name = field.name.to_ascii_lowercase();
                    match name.as_str() {
                        "title" => song.title = Some(trimmed),
                        "artist" => song.artist = Some(trimmed),
                        "album" => song.album = Some(trimmed),
                        "album artist" | "albumartist" => song.album_artist = Some(trimmed),
                        "year" | "date" => song.date = Some(trimmed),
                        "track" | "tracknumber" => song.track = Some(trimmed),
                        "disc" | "discnumber" => song.disc = Some(trimmed),
                        "genre" => song.genre = Some(trimmed),
                        "composer" => song.composer = Some(trimmed),
                        "performer" => song.performer = Some(trimmed),
                        "label" => song.label = Some(trimmed),
                        _ => {
                            song.tags.entry(field.name.clone()).or_insert_with(|| trimmed);
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
        self.song_repository.save(&song)?;
        self.album_repository.update_from_song(song)?;
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
        let text = text.trim_end_matches('\0').trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }

    /// Scan an SACD ISO file and create one `Song` entry per audio track using virtual paths.
    fn scan_sacd_iso_file(&self, file_path: &Path, settings: &MetadataStoreSettings, file_modification_date: DateTime<Utc>) -> Result<()> {
        let path_str = file_path.to_str().ok_or_else(|| Error::msg("SACD ISO path is not valid UTF-8"))?;
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

        let tracks = read_tracks(&mut file, mode, area).map_err(|e| Error::msg(format!("Failed to read SACD track list: {e}")))?;

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
            self.song_repository.save(&song)?;
            self.album_repository.update_from_song(song)?;
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

        let path_str = file_path.to_str().ok_or_else(|| Error::msg("File path is not valid UTF-8"))?;
        let song = Self::read_song(&ScanJob::Local(path_str.to_string()), settings)?;
        self.save_song(song)
    }

    /// Probes `source` with Symphonia and returns its song keyed `file_p`,
    /// with the artwork written to [`ARTWORK_DIR`].
    fn probe_song(source: Box<dyn MediaSource>, ext: Option<&str>, file_p: &str, file_modification_date: DateTime<Utc>) -> Result<Song> {
        let mss = MediaSourceStream::new(source, symphonia::core::io::MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext_str) = ext {
            hint.with_extension(&ext_str.to_lowercase());
        }
        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();

        let probe_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::build_probe().probe(&hint, mss, format_opts, metadata_opts)
        }))
        .map_err(|_| Error::msg(format!("symphonia panicked while probing: {file_p}")))
        .and_then(|r| r.map_err(|e| Error::msg(format!("Error:{file_p} {e}"))));
        match probe_result {
            Ok(mut probed) => {
                let (mut song, image_data) = AudioMetadataExtractor::extract(&mut *probed);
                if let Some(image_data) = &image_data {
                    let image_id = uuid::Uuid::new_v4();
                    if let Err(e) = std::fs::write(Path::new(ARTWORK_DIR).join(image_id.to_string()), &image_data.data) {
                        warn!("Error writing image file: {e}");
                    } else {
                        song.image_id = Some(image_id.to_string());
                    }
                }

                song.file = file_p.to_string();
                song.file_date = file_modification_date;
                Ok(song)
            }
            Err(err) => {
                warn!("{err}");
                Err(err)
            }
        }
    }
}

fn is_supported_file(path: &Path, supported_exts: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| supported_exts.contains(&e.to_lowercase()))
}

/// Key of a favorite station record in the `radio_stations` keyspace; kept
/// apart from the uuid keys of hand-added stations.
fn favorite_station_key(uuid: &str) -> String {
    format!("fav_{uuid}")
}

#[cfg(test)]
mod scan_state_tests {
    use super::*;

    #[test]
    fn a_leftover_scan_marker_becomes_an_interrupted_summary() {
        let dir = format!("/tmp/rsptest_scan_state_{}", random_string::generate(12, "abcdefghijklmnop"));
        {
            let db = Database::builder(&dir).open().expect("open test db");
            let scan_state = db.keyspace("scan_state", KeyspaceCreateOptions::default).expect("keyspace");
            MetadataService::note_interrupted_scan(&scan_state);
            assert!(scan_state.get(LAST_SUMMARY_KEY).unwrap().is_none(), "no marker, nothing to note");

            scan_state.insert(SCAN_RUNNING_KEY, b"1").unwrap();
            MetadataService::note_interrupted_scan(&scan_state);
            let summary: ScanSummary = serde_json::from_slice(&scan_state.get(LAST_SUMMARY_KEY).unwrap().unwrap()).unwrap();
            assert!(!summary.complete);
            assert!(summary.text.contains("Interrupted"), "{}", summary.text);
            assert!(scan_state.get(SCAN_RUNNING_KEY).unwrap().is_none(), "marker cleared");
        }
        _ = std::fs::remove_dir_all(&dir);
    }
}
