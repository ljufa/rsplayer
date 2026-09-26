//! [`PodcastService`] — the podcast domain behind `PodcastCommand`.
//!
//! * A `podcast-worker` thread owns all network I/O (directory search, feed
//!   fetch/refresh) so the sequential command loop never blocks on it; the
//!   command handler only enqueues [`PodcastJob`]s. Between jobs the worker
//!   wakes every minute and refreshes feeds older than the configured
//!   interval, using conditional requests (`ETag` / `Last-Modified`).
//! * A tokio task follows the broadcast events: when the current song is a
//!   podcast episode it persists the position every few seconds (and at
//!   once on pause/stop/track change) and flips `played` past the
//!   configured threshold.
//! * [`PodcastService::resume_position`] answers the player's
//!   `ResumePositionProvider` so a started episode continues where it was.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender as JobSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use fjall::Database;
use log::{debug, error, info, warn};
use tokio::sync::broadcast::{Sender, error::RecvError};

use api_models::player::Song;
use api_models::podcast::{Episode, EpisodePage, Podcast};
use api_models::settings::PodcastSettings;
use api_models::state::{PlayerState, StateChangeEvent};
use config::ArcConfiguration;

use crate::directory::{directory_agent, directory_for};
use crate::feed::{ParsedFeed, parse_feed, stable_id};
use crate::repository::{ArcPodcastRepository, FjallPodcastRepository};

/// How often the worker wakes to look for feeds due for refresh.
const WORKER_TICK: Duration = Duration::from_secs(60);
/// Minimum interval between persisted position updates while playing.
const PROGRESS_WRITE_INTERVAL: Duration = Duration::from_secs(10);
/// Positions this close to the end are not resumed (start over instead).
const RESUME_TAIL_SECS: u64 = 20;
const FEED_LIMIT_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Debug)]
pub enum PodcastJob {
    Search(String),
    Subscribe(String),
    /// One podcast id, or all subscriptions when `None`.
    Refresh(Option<String>),
}

pub struct PodcastService {
    repo: ArcPodcastRepository,
    config: ArcConfiguration,
    changes_tx: Sender<StateChangeEvent>,
    jobs_tx: JobSender<PodcastJob>,
    jobs_rx: Mutex<Option<Receiver<PodcastJob>>>,
}

impl PodcastService {
    pub fn new(db: &Database, config: ArcConfiguration, changes_tx: Sender<StateChangeEvent>) -> Arc<Self> {
        Self::with_repository(Arc::new(FjallPodcastRepository::new(db)), config, changes_tx)
    }

    pub fn with_repository(repo: ArcPodcastRepository, config: ArcConfiguration, changes_tx: Sender<StateChangeEvent>) -> Arc<Self> {
        let (jobs_tx, jobs_rx) = mpsc::channel();
        Arc::new(Self {
            repo,
            config,
            changes_tx,
            jobs_tx,
            jobs_rx: Mutex::new(Some(jobs_rx)),
        })
    }

    /// Spawns the worker thread and the progress tracker task. Call once,
    /// from inside a tokio runtime.
    pub fn start(self: &Arc<Self>) {
        let Some(rx) = self.jobs_rx.lock().expect("lock poisoned").take() else {
            warn!("Podcast service already started");
            return;
        };
        let worker = self.clone();
        thread::Builder::new()
            .name("podcast-worker".into())
            .spawn(move || worker.run_worker(&rx))
            .expect("Failed to spawn podcast-worker thread");

        let tracker_svc = self.clone();
        let mut events = self.changes_tx.subscribe();
        tokio::task::spawn(async move {
            let mut tracker = ProgressTracker::new(tracker_svc.repo.clone(), tracker_svc.changes_tx.clone());
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(RecvError::Lagged(skipped)) => {
                        warn!("Podcast tracker lagged, {skipped} events skipped");
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                };
                let threshold = tracker_svc.settings().played_threshold_percent;
                match event {
                    StateChangeEvent::CurrentSongEvent(song) => tracker.on_current_song(&song),
                    StateChangeEvent::SongTimeEvent(progress) => {
                        tracker.on_time(progress.current_time.as_secs(), progress.total_time.as_secs(), threshold, Instant::now());
                    }
                    StateChangeEvent::PlaybackStateEvent(PlayerState::PAUSED | PlayerState::STOPPED) => tracker.flush(),
                    _ => {}
                }
            }
        });
        info!("Podcast service started");
    }

    fn settings(&self) -> PodcastSettings {
        self.config.get_settings().podcast_settings
    }

    pub fn enqueue(&self, job: PodcastJob) {
        if let Err(e) = self.jobs_tx.send(job) {
            error!("Podcast worker is gone, dropping job: {e}");
        }
    }

    // ----------------------------------------------------------- queries

    #[must_use]
    pub fn list_podcasts(&self) -> Vec<Podcast> {
        self.repo.list_podcasts()
    }

    #[must_use]
    pub fn episodes_page(&self, podcast_id: &str, offset: usize, limit: usize) -> EpisodePage {
        let (total, episodes) = self.repo.episodes_page(podcast_id, offset, limit.clamp(1, 500));
        EpisodePage {
            podcast_id: podcast_id.to_string(),
            total,
            offset,
            episodes,
        }
    }

    #[must_use]
    pub fn episode_with_podcast(&self, episode_id: &str) -> Option<(Episode, Podcast)> {
        let episode = self.repo.get_episode(episode_id)?;
        let podcast = self.repo.get_podcast(&episode.podcast_id)?;
        Some((episode, podcast))
    }

    /// Where playback of `song` should start, when it is an episode with a
    /// saved position that is not (nearly) finished.
    #[must_use]
    pub fn resume_position(&self, song: &Song) -> Option<u64> {
        let episode = self.resolve_episode(song)?;
        if episode.played || episode.position_secs == 0 {
            return None;
        }
        if episode.duration_secs.is_some_and(|d| episode.position_secs + RESUME_TAIL_SECS >= d) {
            return None;
        }
        Some(episode.position_secs)
    }

    /// The episode published after (`newer`) or before `episode_id` in the
    /// same show; with `unplayed_only`, the nearest one not marked played.
    #[must_use]
    pub fn adjacent_episode(&self, episode_id: &str, newer: bool, unplayed_only: bool) -> Option<Episode> {
        let current = self.repo.get_episode(episode_id)?;
        // Newest first, so "newer" walks toward the start of the list.
        let (_, episodes) = self.repo.episodes_page(&current.podcast_id, 0, usize::MAX);
        let pos = episodes.iter().position(|e| e.id == current.id)?;
        let wanted = |e: &Episode| !unplayed_only || !e.played;
        if newer {
            episodes.into_iter().take(pos).rev().find(wanted)
        } else {
            episodes.into_iter().skip(pos + 1).find(wanted)
        }
    }

    fn resolve_episode(&self, song: &Song) -> Option<Episode> {
        song.podcast_episode_id()
            .and_then(|id| self.repo.get_episode(id))
            .or_else(|| self.repo.find_episode_by_url(&song.file))
    }

    // ---------------------------------------------------------- mutations

    pub fn set_played(&self, episode_id: &str, played: bool) -> Result<Episode> {
        let mut episode = self.repo.get_episode(episode_id).ok_or_else(|| anyhow!("unknown episode {episode_id}"))?;
        episode.played = played;
        if played {
            episode.position_secs = 0;
        }
        self.repo.save_episode(&episode)?;
        self.refresh_counts(&episode.podcast_id);
        self.emit(StateChangeEvent::PodcastEpisodeUpdatedEvent(episode.clone()));
        self.emit_podcasts();
        Ok(episode)
    }

    pub fn unsubscribe(&self, podcast_id: &str) -> Result<Option<Podcast>> {
        let podcast = self.repo.get_podcast(podcast_id);
        self.repo.delete_podcast(podcast_id)?;
        self.repo.flush();
        self.emit_podcasts();
        Ok(podcast)
    }

    // ------------------------------------------------------------- worker

    fn run_worker(&self, rx: &Receiver<PodcastJob>) {
        info!("Podcast worker thread started");
        // First pass shortly after boot so stale feeds catch up.
        let mut next_tick = Instant::now() + Duration::from_secs(15);
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            match rx.recv_timeout(wait) {
                Ok(job) => self.handle_job(job),
                Err(RecvTimeoutError::Timeout) => {
                    self.refresh_due();
                    next_tick = Instant::now() + WORKER_TICK;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    info!("Podcast worker stopping: service dropped");
                    break;
                }
            }
        }
    }

    fn handle_job(&self, job: PodcastJob) {
        debug!("Podcast job: {job:?}");
        self.emit(StateChangeEvent::PodcastBusyEvent(true));
        match job {
            PodcastJob::Search(term) => self.search(&term),
            PodcastJob::Subscribe(url) => match self.subscribe(&url) {
                Ok(podcast) => self.notify(&format!("Subscribed to {}", podcast.title)),
                Err(e) => self.notify_error(&format!("Subscribe failed: {e:#}")),
            },
            PodcastJob::Refresh(Some(id)) => match self.repo.get_podcast(&id) {
                Some(mut podcast) => {
                    match self.refresh_podcast(&mut podcast, true) {
                        Ok(added) => self.notify(&format!("{}: {added} new episode(s)", podcast.title)),
                        Err(e) => self.notify_error(&format!("Refresh of {} failed: {e:#}", podcast.title)),
                    }
                    self.repo.flush();
                    self.emit_podcasts();
                }
                None => self.notify_error("Podcast not found"),
            },
            PodcastJob::Refresh(None) => {
                let (refreshed, added) = self.refresh_all(true);
                self.notify(&format!("Refreshed {refreshed} podcast(s), {added} new episode(s)"));
            }
        }
        self.emit(StateChangeEvent::PodcastBusyEvent(false));
    }

    fn search(&self, term: &str) {
        let term = term.trim();
        if term.is_empty() {
            self.emit(StateChangeEvent::PodcastSearchResultsEvent(vec![]));
            return;
        }
        let (directory, fell_back) = directory_for(&self.settings());
        if fell_back {
            self.notify_error("Podcast Index credentials missing — searching iTunes instead");
        }
        match directory.search(term) {
            Ok(results) => self.emit(StateChangeEvent::PodcastSearchResultsEvent(results)),
            Err(e) => {
                self.emit(StateChangeEvent::PodcastSearchResultsEvent(vec![]));
                self.notify_error(&format!("Podcast search failed: {e:#}"));
            }
        }
    }

    fn subscribe(&self, feed_url: &str) -> Result<Podcast> {
        let feed_url = feed_url.trim();
        if !feed_url.starts_with("http://") && !feed_url.starts_with("https://") {
            return Err(anyhow!("feed URL must start with http:// or https://"));
        }
        let id = stable_id(feed_url);
        if let Some(existing) = self.repo.get_podcast(&id) {
            return Err(anyhow!("already subscribed to {}", existing.title));
        }
        let mut podcast = Podcast {
            id,
            feed_url: feed_url.to_string(),
            title: feed_url.to_string(),
            subscribed_at: Some(Utc::now()),
            ..Default::default()
        };
        self.refresh_podcast(&mut podcast, true)?;
        self.repo.flush();
        self.emit_podcasts();
        Ok(podcast)
    }

    /// Refreshes feeds whose last refresh is older than the configured interval.
    fn refresh_due(&self) {
        let (refreshed, added) = self.refresh_all(false);
        if refreshed > 0 {
            info!("Podcast auto-refresh: {refreshed} feed(s) checked, {added} new episode(s)");
        }
    }

    /// Returns `(feeds refreshed, episodes added)`.
    fn refresh_all(&self, force: bool) -> (usize, usize) {
        let interval = chrono::Duration::minutes(i64::from(self.settings().refresh_interval_minutes));
        let now = Utc::now();
        let mut refreshed = 0;
        let mut added = 0;
        let mut changed = false;
        for mut podcast in self.repo.list_podcasts() {
            let due = force || podcast.last_refreshed.is_none_or(|t| now - t >= interval);
            if !due {
                continue;
            }
            refreshed += 1;
            match self.refresh_podcast(&mut podcast, false) {
                Ok(n) => added += n,
                Err(e) => warn!("Refresh of '{}' failed: {e:#}", podcast.title),
            }
            changed = true;
        }
        if changed {
            self.repo.flush();
            self.emit_podcasts();
        }
        (refreshed, added)
    }

    /// Fetches the feed (conditionally unless `force`), stores show data and
    /// episodes. The podcast row is saved in every case so the error /
    /// timestamp is visible. Returns the number of new episodes.
    fn refresh_podcast(&self, podcast: &mut Podcast, force: bool) -> Result<usize> {
        let fetch = fetch_feed(&podcast.feed_url, if force { (None, None) } else { (podcast.etag.as_deref(), podcast.last_modified.as_deref()) });
        let outcome = match fetch {
            Ok(FeedFetch::NotModified) => {
                podcast.last_refreshed = Some(Utc::now());
                podcast.last_error = None;
                self.repo.save_podcast(podcast)?;
                return Ok(0);
            }
            Ok(FeedFetch::Fetched { body, etag, last_modified }) => {
                podcast.etag = etag;
                podcast.last_modified = last_modified;
                parse_feed(&body, &podcast.id)
            }
            Err(e) => Err(e),
        };
        let parsed = match outcome {
            Ok(parsed) => parsed,
            Err(e) => {
                podcast.last_refreshed = Some(Utc::now());
                podcast.last_error = Some(format!("{e:#}"));
                self.repo.save_podcast(podcast)?;
                return Err(e);
            }
        };
        let added = self.apply_feed(podcast, parsed)?;
        Ok(added)
    }

    fn apply_feed(&self, podcast: &mut Podcast, parsed: ParsedFeed) -> Result<usize> {
        let added = self.repo.upsert_episodes(&parsed.episodes).context("store episodes")?;
        let keep = self.settings().max_episodes_per_feed as usize;
        let pruned = self.repo.prune_episodes(&podcast.id, keep);
        if pruned > 0 {
            debug!("Pruned {pruned} old episode(s) of '{}'", parsed.title);
        }
        podcast.title = parsed.title;
        podcast.author = parsed.author;
        podcast.description = parsed.description;
        podcast.image_url = parsed.image_url;
        podcast.website = parsed.website;
        podcast.categories = parsed.categories;
        podcast.last_refreshed = Some(Utc::now());
        podcast.last_error = None;
        let (total, unplayed) = self.repo.episode_counts(&podcast.id);
        podcast.episode_count = u32::try_from(total).unwrap_or(u32::MAX);
        podcast.unplayed_count = u32::try_from(unplayed).unwrap_or(u32::MAX);
        self.repo.save_podcast(podcast).context("store podcast")?;
        Ok(added)
    }

    fn refresh_counts(&self, podcast_id: &str) {
        if let Some(mut podcast) = self.repo.get_podcast(podcast_id) {
            let (total, unplayed) = self.repo.episode_counts(podcast_id);
            podcast.episode_count = u32::try_from(total).unwrap_or(u32::MAX);
            podcast.unplayed_count = u32::try_from(unplayed).unwrap_or(u32::MAX);
            if let Err(e) = self.repo.save_podcast(&podcast) {
                error!("Failed to update podcast counts: {e}");
            }
        }
    }

    // ------------------------------------------------------------- events

    fn emit(&self, event: StateChangeEvent) {
        let _ = self.changes_tx.send(event);
    }

    pub fn emit_podcasts(&self) {
        self.emit(StateChangeEvent::PodcastsEvent(self.repo.list_podcasts()));
    }

    fn notify(&self, message: &str) {
        self.emit(StateChangeEvent::NotificationSuccess(message.to_string()));
    }

    fn notify_error(&self, message: &str) {
        warn!("{message}");
        self.emit(StateChangeEvent::NotificationError(message.to_string()));
    }
}

// ------------------------------------------------------------ feed fetch

enum FeedFetch {
    NotModified,
    Fetched {
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

fn fetch_feed(url: &str, validators: (Option<&str>, Option<&str>)) -> Result<FeedFetch> {
    let agent = directory_agent();
    let mut req = agent
        .get(url)
        .header("accept", "application/rss+xml, application/atom+xml, application/xml, text/xml, */*");
    if let Some(etag) = validators.0 {
        req = req.header("if-none-match", etag);
    }
    if let Some(modified) = validators.1 {
        req = req.header("if-modified-since", modified);
    }
    let resp = req.call().with_context(|| format!("fetch {url}"))?;
    let status = resp.status().as_u16();
    if status == 304 {
        return Ok(FeedFetch::NotModified);
    }
    if !(200..300).contains(&status) {
        return Err(anyhow!("feed answered HTTP {status}"));
    }
    let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(ToString::to_string);
    let etag = header("etag");
    let last_modified = header("last-modified");
    let body = resp
        .into_body()
        .into_with_config()
        .limit(FEED_LIMIT_BYTES)
        .read_to_vec()
        .context("read feed body")?;
    Ok(FeedFetch::Fetched { body, etag, last_modified })
}

// ------------------------------------------------------- progress tracker

struct Current {
    episode: Episode,
    position: u64,
    last_write: Instant,
    dirty: bool,
}

/// Turns player events into per-episode `position_secs` / `played` writes.
/// Pure state machine over the repository; time is injected for tests.
pub struct ProgressTracker {
    repo: ArcPodcastRepository,
    changes_tx: Sender<StateChangeEvent>,
    current: Option<Current>,
}

impl ProgressTracker {
    pub fn new(repo: ArcPodcastRepository, changes_tx: Sender<StateChangeEvent>) -> Self {
        Self {
            repo,
            changes_tx,
            current: None,
        }
    }

    /// A new track started: persist the previous episode's position and
    /// start following the new one if it is a podcast episode.
    pub fn on_current_song(&mut self, song: &Song) {
        if self.current.as_ref().is_some_and(|c| c.episode.audio_url == song.file) {
            return;
        }
        self.flush();
        let episode = song
            .podcast_episode_id()
            .and_then(|id| self.repo.get_episode(id))
            .or_else(|| self.repo.find_episode_by_url(&song.file));
        self.current = episode.filter(|e| !e.played).map(|episode| Current {
            position: episode.position_secs,
            episode,
            last_write: Instant::now(),
            dirty: false,
        });
    }

    /// Progress tick. `total_secs` of 0 means the player does not know the
    /// length; the feed duration is used instead.
    pub fn on_time(&mut self, current_secs: u64, total_secs: u64, threshold_percent: u8, now: Instant) {
        let Some(cur) = self.current.as_mut() else { return };
        if current_secs == cur.position {
            return;
        }
        cur.position = current_secs;
        cur.dirty = true;
        let total = if total_secs > 0 { total_secs } else { cur.episode.duration_secs.unwrap_or(0) };
        if total > 0 && current_secs.saturating_mul(100) >= total.saturating_mul(u64::from(threshold_percent)) {
            self.mark_played();
            return;
        }
        if now.duration_since(cur.last_write) >= PROGRESS_WRITE_INTERVAL {
            self.write(now);
        }
    }

    /// Persist immediately (pause/stop/track change).
    pub fn flush(&mut self) {
        self.write(Instant::now());
    }

    fn write(&mut self, now: Instant) {
        let Some(cur) = self.current.as_mut() else { return };
        if !cur.dirty {
            return;
        }
        cur.episode.position_secs = cur.position;
        cur.dirty = false;
        cur.last_write = now;
        match self.repo.save_episode(&cur.episode) {
            Ok(()) => {
                let _ = self.changes_tx.send(StateChangeEvent::PodcastEpisodeUpdatedEvent(cur.episode.clone()));
            }
            Err(e) => error!("Failed to save episode position: {e}"),
        }
    }

    fn mark_played(&mut self) {
        let Some(mut cur) = self.current.take() else { return };
        cur.episode.played = true;
        cur.episode.position_secs = 0;
        info!("Podcast episode finished: {}", cur.episode.title);
        if let Err(e) = self.repo.save_episode(&cur.episode) {
            error!("Failed to mark episode played: {e}");
            return;
        }
        if let Some(mut podcast) = self.repo.get_podcast(&cur.episode.podcast_id) {
            let (total, unplayed) = self.repo.episode_counts(&podcast.id);
            podcast.episode_count = u32::try_from(total).unwrap_or(u32::MAX);
            podcast.unplayed_count = u32::try_from(unplayed).unwrap_or(u32::MAX);
            let _ = self.repo.save_podcast(&podcast);
        }
        let _ = self.changes_tx.send(StateChangeEvent::PodcastEpisodeUpdatedEvent(cur.episode));
        let _ = self.changes_tx.send(StateChangeEvent::PodcastsEvent(self.repo.list_podcasts()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::FjallPodcastRepository;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    fn setup() -> (TempDir, ArcPodcastRepository, ProgressTracker, broadcast::Receiver<StateChangeEvent>) {
        let tmp = TempDir::new().unwrap();
        let repo: ArcPodcastRepository = Arc::new(FjallPodcastRepository::new_standalone(&tmp.path().join("db")));
        let podcast = Podcast {
            id: "p".into(),
            title: "P".into(),
            ..Default::default()
        };
        repo.save_podcast(&podcast).unwrap();
        let episode = Episode {
            id: "e1".into(),
            podcast_id: "p".into(),
            title: "E1".into(),
            audio_url: "https://cdn/e1.mp3".into(),
            duration_secs: Some(1000),
            ..Default::default()
        };
        repo.upsert_episodes(&[episode]).unwrap();
        let (tx, rx) = broadcast::channel(64);
        let tracker = ProgressTracker::new(repo.clone(), tx);
        (tmp, repo, tracker, rx)
    }

    fn song(url: &str, tag: Option<&str>) -> Song {
        let mut song = Song {
            file: url.into(),
            ..Default::default()
        };
        if let Some(id) = tag {
            song.tags.insert(api_models::podcast::EPISODE_ID_TAG.into(), id.into());
        }
        song
    }

    #[test]
    fn writes_position_throttled_and_on_flush() {
        let (_t, repo, mut tracker, mut rx) = setup();
        let t0 = Instant::now();
        tracker.on_current_song(&song("https://cdn/e1.mp3", None));
        tracker.on_time(5, 1000, 95, t0 + Duration::from_secs(1));
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 0, "not yet written");
        tracker.on_time(12, 1000, 95, t0 + PROGRESS_WRITE_INTERVAL + Duration::from_secs(1));
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 12);
        assert!(matches!(rx.try_recv(), Ok(StateChangeEvent::PodcastEpisodeUpdatedEvent(e)) if e.position_secs == 12));
        tracker.on_time(15, 1000, 95, t0 + PROGRESS_WRITE_INTERVAL + Duration::from_secs(2));
        tracker.flush();
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 15);
    }

    #[test]
    fn track_change_flushes_and_ignores_non_episodes() {
        let (_t, repo, mut tracker, _rx) = setup();
        tracker.on_current_song(&song("https://cdn/e1.mp3", Some("e1")));
        tracker.on_time(30, 1000, 95, Instant::now());
        tracker.on_current_song(&song("music/song.flac", None));
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 30);
        tracker.on_time(500, 1000, 95, Instant::now() + Duration::from_secs(60));
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 30, "non-episode must not touch e1");
    }

    #[test]
    fn marks_played_at_threshold_using_feed_duration_when_player_has_none() {
        let (_t, repo, mut tracker, mut rx) = setup();
        tracker.on_current_song(&song("https://cdn/e1.mp3", Some("e1")));
        tracker.on_time(949, 0, 95, Instant::now());
        assert!(!repo.get_episode("e1").unwrap().played);
        tracker.on_time(950, 0, 95, Instant::now());
        let ep = repo.get_episode("e1").unwrap();
        assert!(ep.played);
        assert_eq!(ep.position_secs, 0);
        assert_eq!(repo.get_podcast("p").unwrap().unplayed_count, 0);
        let mut saw_update = false;
        let mut saw_podcasts = false;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                StateChangeEvent::PodcastEpisodeUpdatedEvent(e) => saw_update = e.played,
                StateChangeEvent::PodcastsEvent(_) => saw_podcasts = true,
                _ => {}
            }
        }
        assert!(saw_update && saw_podcasts);
        // Further ticks of the finished episode are ignored.
        tracker.on_time(960, 0, 95, Instant::now());
        tracker.flush();
        assert_eq!(repo.get_episode("e1").unwrap().position_secs, 0);
    }

    #[test]
    fn resume_position_rules() {
        let (_t, repo, _tracker, _rx) = setup();
        let tmp = TempDir::new().unwrap();
        let db = Database::builder(tmp.path().join("cfg")).open().unwrap();
        let config = config::Configuration::new(&db, api_models::settings::Settings::default());
        let (tx, _rx) = broadcast::channel(8);
        let svc = PodcastService::with_repository(repo.clone(), config, tx);

        let s = song("https://cdn/e1.mp3", Some("e1"));
        assert_eq!(svc.resume_position(&s), None, "never started");
        let mut ep = repo.get_episode("e1").unwrap();
        ep.position_secs = 300;
        repo.save_episode(&ep).unwrap();
        assert_eq!(svc.resume_position(&s), Some(300));
        assert_eq!(svc.resume_position(&song("https://cdn/e1.mp3", None)), Some(300), "url fallback");
        ep.position_secs = 990;
        repo.save_episode(&ep).unwrap();
        assert_eq!(svc.resume_position(&s), None, "within the tail → start over");
        ep.position_secs = 300;
        ep.played = true;
        repo.save_episode(&ep).unwrap();
        assert_eq!(svc.resume_position(&s), None, "played → start over");

        let again = svc.set_played("e1", false).unwrap();
        assert!(!again.played);
        assert_eq!(svc.resume_position(&s), Some(300));
        assert_eq!(svc.list_podcasts()[0].unplayed_count, 1);
    }

    #[test]
    fn adjacent_episode_walks_by_publish_date() {
        let tmp = TempDir::new().unwrap();
        let repo: ArcPodcastRepository = Arc::new(FjallPodcastRepository::new_standalone(&tmp.path().join("db")));
        let ep = |n: u32, played: bool| Episode {
            id: format!("e{n}"),
            podcast_id: "p".into(),
            title: format!("E{n}"),
            audio_url: format!("https://cdn/e{n}.mp3"),
            published: chrono::DateTime::from_timestamp(i64::from(n) * 86_400, 0),
            played,
            ..Default::default()
        };
        repo.upsert_episodes(&[ep(1, false), ep(2, true), ep(3, false), ep(4, false)]).unwrap();
        let cfg_tmp = TempDir::new().unwrap();
        let db = Database::builder(cfg_tmp.path().join("cfg")).open().unwrap();
        let config = config::Configuration::new(&db, api_models::settings::Settings::default());
        let (tx, _rx) = broadcast::channel(8);
        let svc = PodcastService::with_repository(repo, config, tx);

        let id = |e: Option<Episode>| e.map(|e| e.id);
        assert_eq!(id(svc.adjacent_episode("e1", true, true)), Some("e3".into()), "skips played e2");
        assert_eq!(id(svc.adjacent_episode("e1", true, false)), Some("e2".into()));
        assert_eq!(id(svc.adjacent_episode("e4", true, true)), None, "newest has no next");
        assert_eq!(id(svc.adjacent_episode("e3", false, false)), Some("e2".into()));
        assert_eq!(id(svc.adjacent_episode("e1", false, false)), None);
    }
}
