//! [`ScrobbleService`]: feeds player events to the [`Scrobbler`] and sends
//! what it decides, off the event path.
//!
//! Two tasks: the event task only runs the state machine and hands actions
//! to a bounded channel (never blocks, survives `Lagged`), the worker task
//! makes the HTTP calls on the blocking pool, one at a time, and owns the
//! retry queue flushing (after any success, and every minute with backoff
//! while a service fails). Settings are read on each use, so enabling,
//! disabling or changing credentials takes effect without a restart.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use fjall::Database;
use log::{debug, info, warn};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc::{self, error::TrySendError};

use api_models::playback_source::PlaybackSource;
use api_models::scrobble::{DEFAULT_LISTENBRAINZ_URL, ScrobbleCommand, ScrobbleSettings, ScrobbleStatus};
use api_models::state::StateChangeEvent;
use config::ArcConfiguration;

use crate::http::SubmitError;
use crate::lastfm::{self, LastFmClient};
use crate::listenbrainz::{self, ListenBrainzClient};
use crate::machine::{Action, Scrobbler};
use crate::queue::{RetryQueue, Service};
use crate::track::{Listen, ScrobbleTrack};

const JOB_QUEUE: usize = 64;
const FLUSH_TICK: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(3600);
/// At most one "credentials rejected" notification per service this often.
const AUTH_NOTICE_EVERY: Duration = Duration::from_secs(3600);

/// Service base URLs; tests point them at a local server.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub lastfm_api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            lastfm_api: lastfm::API_URL.to_string(),
        }
    }
}

enum Job {
    Action(Action),
    Command(ScrobbleCommand),
}

pub struct ScrobbleService {
    config: ArcConfiguration,
    changes_tx: broadcast::Sender<StateChangeEvent>,
    queue: RetryQueue,
    follower_active: Arc<AtomicBool>,
    endpoints: Endpoints,
    jobs_tx: mpsc::Sender<Job>,
    jobs_rx: Mutex<Option<mpsc::Receiver<Job>>>,
    started: AtomicBool,
}

impl ScrobbleService {
    /// `follower_active` is the flag set while this instance plays as a
    /// multiroom follower (the leader scrobbles then).
    pub fn new(
        db: &Database,
        config: ArcConfiguration,
        changes_tx: broadcast::Sender<StateChangeEvent>,
        follower_active: Arc<AtomicBool>,
    ) -> Result<Arc<Self>> {
        Self::with_endpoints(db, config, changes_tx, follower_active, Endpoints::default())
    }

    pub fn with_endpoints(
        db: &Database,
        config: ArcConfiguration,
        changes_tx: broadcast::Sender<StateChangeEvent>,
        follower_active: Arc<AtomicBool>,
        endpoints: Endpoints,
    ) -> Result<Arc<Self>> {
        let (jobs_tx, jobs_rx) = mpsc::channel(JOB_QUEUE);
        Ok(Arc::new(Self {
            config,
            changes_tx,
            queue: RetryQueue::new(db)?,
            follower_active,
            endpoints,
            jobs_tx,
            jobs_rx: Mutex::new(Some(jobs_rx)),
            started: AtomicBool::new(false),
        }))
    }

    /// Starts the event and worker tasks; `source` is what the player plays
    /// from right now (later changes arrive as events). Needs a tokio runtime.
    pub fn start(self: &Arc<Self>, source: &PlaybackSource) {
        let Some(jobs_rx) = self.jobs_rx.lock().expect("lock poisoned").take() else {
            return;
        };
        self.started.store(true, Ordering::Relaxed);
        let mut events = self.changes_tx.subscribe();
        let mut machine = Scrobbler::new(source);
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if let Some(action) = machine.handle(&event, unix_now()) {
                            svc.dispatch(action);
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        debug!("Scrobble event receiver lagged, {skipped} events skipped");
                        machine.on_lagged();
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
        let worker = Worker::new(self.clone());
        tokio::spawn(worker.run(jobs_rx));
    }

    /// Runs a UI command on the worker; results come back as events.
    pub fn handle_command(&self, cmd: ScrobbleCommand) {
        if !self.started.load(Ordering::Relaxed) {
            self.send(StateChangeEvent::NotificationError("Scrobbling is not available here (demo mode).".into()));
            return;
        }
        if self.jobs_tx.try_send(Job::Command(cmd)).is_err() {
            self.send(StateChangeEvent::NotificationError("Scrobbling is busy, try again in a moment.".into()));
        }
    }

    fn dispatch(&self, action: Action) {
        if self.follower_active.load(Ordering::Relaxed) {
            return;
        }
        let settings = self.config.get_settings();
        // Browser playback: the server sees no progress, the browser plays.
        if settings.local_browser_playback || ready_services(&settings.scrobble_settings).is_empty() {
            return;
        }
        match self.jobs_tx.try_send(Job::Action(action)) {
            Ok(()) => {}
            // Worker stuck on a slow server: keep scrobbles, drop now playing.
            Err(TrySendError::Full(Job::Action(Action::Scrobble(listen)))) => {
                for service in ready_services(&settings.scrobble_settings) {
                    if let Err(e) = self.queue.push(service, &listen) {
                        warn!("Failed to queue scrobble: {e}");
                    }
                }
            }
            Err(_) => debug!("Scrobble worker busy, now playing update dropped"),
        }
    }

    fn send(&self, event: StateChangeEvent) {
        self.changes_tx.send(event).ok();
    }

    fn scrobble_settings(&self) -> ScrobbleSettings {
        self.config.get_settings().scrobble_settings
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Services that are enabled and have what they need to send.
fn ready_services(s: &ScrobbleSettings) -> Vec<Service> {
    let mut out = Vec::with_capacity(2);
    if s.listenbrainz_enabled && !s.listenbrainz_token.trim().is_empty() {
        out.push(Service::ListenBrainz);
    }
    if s.lastfm_enabled && !s.lastfm_api_key.is_empty() && !s.lastfm_api_secret.is_empty() && !s.lastfm_session_key.is_empty() {
        out.push(Service::LastFm);
    }
    out
}

fn listenbrainz_url(s: &ScrobbleSettings) -> String {
    let url = s.listenbrainz_url.trim();
    if url.is_empty() { DEFAULT_LISTENBRAINZ_URL.to_string() } else { url.to_string() }
}

/// Runs a blocking call on the blocking pool.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, SubmitError> + Send + 'static) -> Result<T, SubmitError> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(SubmitError::Retry(format!("task failed: {e}"))))
}

struct Backoff {
    delay: Duration,
    next: Instant,
}

impl Backoff {
    fn new() -> Self {
        Self {
            delay: FLUSH_TICK,
            next: Instant::now(),
        }
    }

    fn fail(&mut self) {
        self.next = Instant::now() + self.delay;
        self.delay = (self.delay * 2).min(MAX_BACKOFF);
    }

    fn reset(&mut self) {
        self.delay = FLUSH_TICK;
        self.next = Instant::now();
    }

    fn due(&self) -> bool {
        Instant::now() >= self.next
    }
}

struct PendingAuth {
    api_key: String,
    api_secret: String,
    token: String,
    url: String,
}

struct Worker {
    svc: Arc<ScrobbleService>,
    backoff: HashMap<Service, Backoff>,
    auth_notice: HashMap<Service, Instant>,
    pending_auth: Option<PendingAuth>,
    listenbrainz_user: Option<String>,
}

impl Worker {
    fn new(svc: Arc<ScrobbleService>) -> Self {
        Self {
            svc,
            backoff: Service::ALL.iter().map(|s| (*s, Backoff::new())).collect(),
            auth_notice: HashMap::new(),
            pending_auth: None,
            listenbrainz_user: None,
        }
    }

    async fn run(mut self, mut jobs: mpsc::Receiver<Job>) {
        let mut tick = tokio::time::interval(FLUSH_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                job = jobs.recv() => match job {
                    Some(Job::Action(Action::NowPlaying(track))) => self.now_playing(track).await,
                    Some(Job::Action(Action::Scrobble(listen))) => self.scrobble(listen).await,
                    Some(Job::Command(cmd)) => self.command(cmd).await,
                    None => break,
                },
                _ = tick.tick() => self.flush_due().await,
            }
        }
    }

    fn ready(&self) -> Vec<Service> {
        ready_services(&self.svc.scrobble_settings())
    }

    async fn now_playing(&mut self, track: ScrobbleTrack) {
        for service in self.ready() {
            let s = self.svc.scrobble_settings();
            let track = track.clone();
            let result = match service {
                Service::ListenBrainz => {
                    let client = ListenBrainzClient::new(&listenbrainz_url(&s), &s.listenbrainz_token);
                    blocking(move || client.playing_now(&track)).await
                }
                Service::LastFm => {
                    let client = self.lastfm_client(&s);
                    let sk = s.lastfm_session_key;
                    blocking(move || client.update_now_playing(&sk, &track)).await
                }
            };
            match result {
                Ok(()) => self.succeeded(service).await,
                Err(e) => self.failed(service, &e),
            }
        }
    }

    async fn scrobble(&mut self, listen: Listen) {
        for service in self.ready() {
            // Older ones first: queue behind them.
            if !self.svc.queue.is_empty(service) {
                self.enqueue(service, &listen);
                if self.backoff[&service].due() {
                    self.flush(service).await;
                }
                continue;
            }
            match self.submit(service, vec![listen.clone()]).await {
                Ok(()) => {
                    debug!("Scrobbled to {}", service.name());
                    self.backoff_mut(service).reset();
                }
                Err(e) => {
                    if e.keep() {
                        self.enqueue(service, &listen);
                    }
                    self.failed(service, &e);
                }
            }
        }
    }

    fn enqueue(&self, service: Service, listen: &Listen) {
        if let Err(e) = self.svc.queue.push(service, listen) {
            warn!("Failed to queue scrobble for {}: {e}", service.name());
        }
    }

    fn backoff_mut(&mut self, service: Service) -> &mut Backoff {
        self.backoff.entry(service).or_insert_with(Backoff::new)
    }

    fn lastfm_client(&self, s: &ScrobbleSettings) -> LastFmClient {
        LastFmClient::new(&self.svc.endpoints.lastfm_api, &s.lastfm_api_key, &s.lastfm_api_secret)
    }

    async fn submit(&self, service: Service, listens: Vec<Listen>) -> Result<(), SubmitError> {
        let s = self.svc.scrobble_settings();
        match service {
            Service::ListenBrainz => {
                let client = ListenBrainzClient::new(&listenbrainz_url(&s), &s.listenbrainz_token);
                blocking(move || client.submit(&listens)).await
            }
            Service::LastFm => {
                let client = self.lastfm_client(&s);
                let sk = s.lastfm_session_key;
                blocking(move || client.scrobble(&sk, &listens)).await
            }
        }
    }

    /// A call went through: the service is reachable, send what waits.
    async fn succeeded(&mut self, service: Service) {
        self.backoff_mut(service).reset();
        if !self.svc.queue.is_empty(service) {
            self.flush(service).await;
        }
    }

    fn failed(&mut self, service: Service, e: &SubmitError) {
        self.backoff_mut(service).fail();
        match e {
            SubmitError::Retry(m) => info!("{} unreachable, will retry: {m}", service.name()),
            SubmitError::Rejected(m) => warn!("{} rejected a request: {m}", service.name()),
            SubmitError::Auth(m) => {
                warn!("{} rejected the credentials: {m}", service.name());
                let due = self.auth_notice.get(&service).is_none_or(|t| t.elapsed() >= AUTH_NOTICE_EVERY);
                if due {
                    self.auth_notice.insert(service, Instant::now());
                    let what = match service {
                        Service::ListenBrainz => "the token",
                        Service::LastFm => "the API key or secret",
                    };
                    self.notify_error(&format!(
                        "{} rejected {what}. Scrobbles are kept until it is fixed in Settings.",
                        service.name()
                    ));
                }
            }
            SubmitError::InvalidSession => {
                warn!("Last.fm session is no longer valid, disconnecting");
                self.store_session(None);
                self.notify_error("The Last.fm session is no longer valid. Connect again in Settings, scrobbles are kept until then.");
                self.emit_status();
            }
        }
    }

    /// Sends queued scrobbles oldest first, in batches, until the queue is
    /// empty or the service fails.
    async fn flush(&mut self, service: Service) {
        let batch_size = match service {
            Service::ListenBrainz => listenbrainz::BATCH_SIZE,
            Service::LastFm => lastfm::BATCH_SIZE,
        };
        let mut sent = 0;
        loop {
            if !self.ready().contains(&service) {
                break;
            }
            let batch = self.svc.queue.batch(service, batch_size);
            if batch.is_empty() {
                break;
            }
            let (keys, listens): (Vec<Vec<u8>>, Vec<Listen>) = batch.into_iter().map(|e| (e.key, e.listen)).unzip();
            match self.submit(service, listens.clone()).await {
                Ok(()) => {
                    sent += keys.len();
                    self.svc.queue.remove(&keys);
                }
                // One bad scrobble must not hold back the others: send them
                // one by one and drop only the refused ones.
                Err(SubmitError::Rejected(m)) if keys.len() > 1 => {
                    debug!("{} refused a batch ({m}), sending one by one", service.name());
                    for (key, listen) in keys.into_iter().zip(listens) {
                        match self.submit(service, vec![listen]).await {
                            Ok(()) => sent += 1,
                            Err(SubmitError::Rejected(m)) => warn!("{} refused a queued scrobble, dropped: {m}", service.name()),
                            Err(e) => {
                                self.failed(service, &e);
                                self.finish_flush(service, sent);
                                return;
                            }
                        }
                        self.svc.queue.remove(&[key]);
                    }
                }
                Err(SubmitError::Rejected(m)) => {
                    warn!("{} refused a queued scrobble, dropped: {m}", service.name());
                    self.svc.queue.remove(&keys);
                }
                Err(e) => {
                    self.failed(service, &e);
                    self.finish_flush(service, sent);
                    return;
                }
            }
        }
        self.backoff_mut(service).reset();
        self.finish_flush(service, sent);
    }

    fn finish_flush(&self, service: Service, sent: usize) {
        if sent > 0 {
            info!("Sent {sent} queued scrobble(s) to {}", service.name());
            self.emit_status();
        }
    }

    async fn flush_due(&mut self) {
        for service in self.ready() {
            if !self.svc.queue.is_empty(service) && self.backoff[&service].due() {
                self.flush(service).await;
            }
        }
    }

    async fn command(&mut self, cmd: ScrobbleCommand) {
        match cmd {
            ScrobbleCommand::QueryStatus => {}
            ScrobbleCommand::TestListenBrainz { url, token } => self.test_listenbrainz(&url, &token).await,
            ScrobbleCommand::LastFmStartAuth { api_key, api_secret } => self.lastfm_start_auth(api_key, api_secret).await,
            ScrobbleCommand::LastFmCompleteAuth => self.lastfm_complete_auth().await,
            ScrobbleCommand::LastFmDisconnect => {
                self.pending_auth = None;
                self.store_session(None);
                self.notify("Disconnected from Last.fm");
            }
        }
        self.emit_status();
    }

    async fn test_listenbrainz(&mut self, url: &str, token: &str) {
        let stored = self.svc.scrobble_settings();
        let token = if ScrobbleSettings::is_masked(token) { stored.listenbrainz_token } else { token.trim().to_string() };
        if token.is_empty() {
            self.notify_error("Enter a ListenBrainz token first.");
            return;
        }
        let url = if url.trim().is_empty() { DEFAULT_LISTENBRAINZ_URL } else { url };
        let client = ListenBrainzClient::new(url, &token);
        match blocking(move || client.validate_token()).await {
            Ok(user) => {
                self.notify(&format!("ListenBrainz token is valid (user {user})"));
                self.listenbrainz_user = Some(user);
            }
            Err(e) => {
                self.listenbrainz_user = None;
                self.notify_error(&format!("ListenBrainz test failed: {e}"));
            }
        }
    }

    async fn lastfm_start_auth(&mut self, api_key: String, api_secret: String) {
        let stored = self.svc.scrobble_settings();
        let api_secret = if ScrobbleSettings::is_masked(&api_secret) { stored.lastfm_api_secret } else { api_secret };
        let (api_key, api_secret) = (api_key.trim().to_string(), api_secret.trim().to_string());
        if api_key.is_empty() || api_secret.is_empty() {
            self.notify_error("Enter your Last.fm API key and shared secret first.");
            return;
        }
        let client = LastFmClient::new(&self.svc.endpoints.lastfm_api, &api_key, &api_secret);
        match blocking(move || client.get_token()).await {
            Ok(token) => {
                let url = lastfm::auth_page_url(&api_key, &token);
                self.pending_auth = Some(PendingAuth {
                    api_key,
                    api_secret,
                    token,
                    url,
                });
                self.notify("Allow access on the Last.fm page, then press Finish connecting.");
            }
            Err(e) => self.notify_error(&format!("Could not start connecting to Last.fm: {e}")),
        }
    }

    async fn lastfm_complete_auth(&mut self) {
        let Some(pending) = self.pending_auth.as_ref() else {
            self.notify_error("Press Connect first.");
            return;
        };
        let client = LastFmClient::new(&self.svc.endpoints.lastfm_api, &pending.api_key, &pending.api_secret);
        let token = pending.token.clone();
        match blocking(move || client.get_session(&token)).await {
            Ok((name, session_key)) => {
                let pending = self.pending_auth.take().expect("checked above");
                let mut settings = self.svc.config.get_settings();
                let s = &mut settings.scrobble_settings;
                s.lastfm_api_key = pending.api_key;
                s.lastfm_api_secret = pending.api_secret;
                s.lastfm_session_key = session_key;
                s.lastfm_username.clone_from(&name);
                self.svc.config.save_settings(&settings);
                info!("Connected to Last.fm as {name}");
                self.notify(&format!("Connected to Last.fm as {name}"));
                self.auth_notice.remove(&Service::LastFm);
                self.succeeded(Service::LastFm).await;
            }
            Err(SubmitError::Rejected(m)) if m.contains("error 14") => {
                self.notify_error("Last.fm has not been granted access yet. Allow it on the Last.fm page, then press Finish connecting.");
            }
            Err(e) => {
                if matches!(&e, SubmitError::Rejected(m) if m.contains("error 15")) {
                    self.pending_auth = None;
                }
                self.notify_error(&format!("Could not connect to Last.fm: {e}"));
            }
        }
    }

    /// Stores a Last.fm session `(user, key)`, or clears it.
    fn store_session(&self, session: Option<(String, String)>) {
        let mut settings = self.svc.config.get_settings();
        let (user, key) = session.unwrap_or_default();
        settings.scrobble_settings.lastfm_username = user;
        settings.scrobble_settings.lastfm_session_key = key;
        self.svc.config.save_settings(&settings);
    }

    fn status(&self) -> ScrobbleStatus {
        ScrobbleStatus {
            lastfm_username: self.svc.scrobble_settings().lastfm_username,
            lastfm_auth_url: self.pending_auth.as_ref().map(|p| p.url.clone()),
            listenbrainz_username: self.listenbrainz_user.clone(),
            listenbrainz_queued: self.svc.queue.len(Service::ListenBrainz),
            lastfm_queued: self.svc.queue.len(Service::LastFm),
        }
    }

    fn emit_status(&self) {
        self.svc.send(StateChangeEvent::ScrobbleStatusEvent(self.status()));
    }

    fn notify(&self, message: &str) {
        self.svc.send(StateChangeEvent::NotificationSuccess(message.to_string()));
    }

    fn notify_error(&self, message: &str) {
        self.svc.send(StateChangeEvent::NotificationError(message.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockServer;
    use api_models::player::Song;
    use api_models::settings::Settings;
    use api_models::state::{PlayerState, SongProgress};
    use config::Configuration;
    use tempfile::TempDir;

    struct Setup {
        _tmp: TempDir,
        svc: Arc<ScrobbleService>,
        tx: broadcast::Sender<StateChangeEvent>,
        config: ArcConfiguration,
    }

    fn setup(lb: Option<&MockServer>, lastfm: Option<&MockServer>) -> Setup {
        let tmp = TempDir::new().unwrap();
        let db = Database::builder(tmp.path().join("db")).open().unwrap();
        let mut settings = Settings::default();
        if let Some(server) = lb {
            settings.scrobble_settings.listenbrainz_enabled = true;
            settings.scrobble_settings.listenbrainz_token = "lb-token".into();
            settings.scrobble_settings.listenbrainz_url.clone_from(&server.url);
        }
        if lastfm.is_some() {
            settings.scrobble_settings.lastfm_enabled = true;
            settings.scrobble_settings.lastfm_api_key = "key".into();
            settings.scrobble_settings.lastfm_api_secret = "secret".into();
            settings.scrobble_settings.lastfm_session_key = "sk".into();
            settings.scrobble_settings.lastfm_username = "alice".into();
        }
        let config = Configuration::new(&db, settings);
        let (tx, _) = broadcast::channel(256);
        let endpoints = Endpoints {
            lastfm_api: lastfm.map_or_else(|| "http://127.0.0.1:1".to_string(), |s| format!("{}/2.0/", s.url)),
        };
        let svc = ScrobbleService::with_endpoints(&db, config.clone(), tx.clone(), Arc::new(AtomicBool::new(false)), endpoints).unwrap();
        Setup {
            _tmp: tmp,
            svc,
            tx,
            config,
        }
    }

    fn listen() -> Listen {
        Listen {
            track: ScrobbleTrack {
                artist: "A".into(),
                title: "T".into(),
                ..Default::default()
            },
            listened_at: 1_700_000_000,
        }
    }

    async fn wait_for(server: &MockServer, n: usize) {
        for _ in 0..200 {
            if server.requests().len() >= n {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("expected {n} requests, got {:?}", server.requests());
    }

    #[tokio::test]
    async fn player_events_reach_listenbrainz() {
        let server = MockServer::start();
        let s = setup(Some(&server), None);
        s.svc.start(&PlaybackSource::Queue);
        let song = Song {
            artist: Some("Artist".into()),
            title: Some("Title".into()),
            time: Some(Duration::from_secs(60)),
            file: "music/a.flac".into(),
            ..Default::default()
        };
        s.tx.send(StateChangeEvent::CurrentSongEvent(song)).unwrap();
        s.tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING)).unwrap();
        for pos in 0..=31 {
            s.tx.send(StateChangeEvent::SongTimeEvent(SongProgress {
                total_time: Duration::from_secs(60),
                current_time: Duration::from_secs(pos),
            }))
            .unwrap();
        }
        wait_for(&server, 2).await;
        let reqs = server.requests();
        assert_eq!(reqs[0].json()["listen_type"], "playing_now");
        assert_eq!(reqs[1].json()["listen_type"], "single");
        assert_eq!(reqs[1].json()["payload"][0]["track_metadata"]["track_name"], "Title");
    }

    #[tokio::test]
    async fn failed_scrobbles_are_queued_and_flushed_later() {
        let server = MockServer::start();
        server.reply(503, "down");
        let s = setup(Some(&server), None);
        let mut worker = Worker::new(s.svc.clone());
        worker.scrobble(listen()).await;
        assert_eq!(s.svc.queue.len(Service::ListenBrainz), 1);
        // Queued behind the first one while backing off.
        worker.scrobble(listen()).await;
        assert_eq!(s.svc.queue.len(Service::ListenBrainz), 2);
        assert_eq!(server.requests().len(), 1);
        // A now playing that goes through flushes the queue as one import.
        worker.backoff_mut(Service::ListenBrainz).reset();
        worker.now_playing(listen().track).await;
        assert!(s.svc.queue.is_empty(Service::ListenBrainz));
        let reqs = server.requests();
        assert_eq!(reqs.len(), 3);
        assert_eq!(reqs[1].json()["listen_type"], "playing_now");
        assert_eq!(reqs[2].json()["listen_type"], "import");
        assert_eq!(reqs[2].json()["payload"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rejected_scrobble_is_dropped() {
        let server = MockServer::start();
        server.reply(400, r#"{"error":"bad"}"#);
        let s = setup(Some(&server), None);
        let mut worker = Worker::new(s.svc.clone());
        worker.scrobble(listen()).await;
        assert!(s.svc.queue.is_empty(Service::ListenBrainz));
    }

    #[tokio::test]
    async fn invalid_lastfm_session_disconnects_and_keeps_the_scrobble() {
        let server = MockServer::start();
        server.reply(403, r#"{"error":9,"message":"Invalid session key"}"#);
        let s = setup(None, Some(&server));
        let mut events = s.tx.subscribe();
        let mut worker = Worker::new(s.svc.clone());
        worker.scrobble(listen()).await;
        assert_eq!(s.svc.queue.len(Service::LastFm), 1);
        let stored = s.config.get_settings().scrobble_settings;
        assert!(stored.lastfm_session_key.is_empty() && stored.lastfm_username.is_empty());
        assert!(matches!(events.recv().await.unwrap(), StateChangeEvent::NotificationError(m) if m.contains("Last.fm")));
        // Not connected any more: nothing is sent.
        worker.now_playing(listen().track).await;
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn lastfm_connect_flow_stores_the_session() {
        let server = MockServer::start();
        server
            .reply(200, r#"{"token":"tok"}"#)
            .reply(200, r#"{"session":{"name":"bob","key":"new-sk"}}"#);
        let s = setup(None, Some(&server));
        {
            let mut settings = s.config.get_settings_mut();
            let sc = &mut settings.scrobble_settings;
            sc.lastfm_api_secret = "stored-secret".into();
            sc.lastfm_session_key.clear();
            sc.lastfm_username.clear();
        }
        let mut worker = Worker::new(s.svc.clone());
        worker
            .command(ScrobbleCommand::LastFmStartAuth {
                api_key: "key".into(),
                api_secret: api_models::settings::SECRET_MASK.into(),
            })
            .await;
        let url = worker.status().lastfm_auth_url.unwrap();
        assert!(url.starts_with(lastfm::AUTH_URL) && url.contains("token=tok") && url.contains("api_key=key"));
        worker.command(ScrobbleCommand::LastFmCompleteAuth).await;
        let stored = s.config.get_settings().scrobble_settings;
        assert_eq!(stored.lastfm_session_key, "new-sk");
        assert_eq!(stored.lastfm_username, "bob");
        assert_eq!(stored.lastfm_api_secret, "stored-secret");
        assert_eq!(worker.status().lastfm_auth_url, None);
        // The masked secret was replaced by the stored one when signing.
        let first = &server.requests()[0];
        let unsigned: Vec<(String, String)> = first.form().into_iter().filter(|(k, _)| k != "api_sig").collect();
        assert_eq!(first.field("api_sig"), Some(lastfm::sign(&unsigned, "stored-secret")));

        worker.command(ScrobbleCommand::LastFmDisconnect).await;
        assert!(s.config.get_settings().scrobble_settings.lastfm_session_key.is_empty());
    }
}
