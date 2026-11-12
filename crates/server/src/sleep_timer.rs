//! Sleep timer: pause playback after N minutes or at the end of the current
//! track.
//!
//! One task ([`run`]) owns the timer. Commands reach it through a
//! [`SleepTimerHandle`] (from `PlayerCommand::SetSleepTimer` and
//! `QuerySleepTimer`); every change is broadcast as
//! `StateChangeEvent::SleepTimerEvent`, so all clients show the same
//! countdown. A minutes timer carries its end time (`ends_at`) and clients
//! count down locally; nothing is sent while it runs.
//!
//! - `Minutes`: sleeps until the deadline. Setting a new timer replaces the
//!   running one, `Off` cancels it.
//! - `EndOfTrack`: watches the state broadcast and fires when the current
//!   song changes (track ended, Next, another song picked) or playback stops
//!   (end of the queue, a manual pause), whichever comes first. A live radio
//!   stream has no track end, so it is rejected there.
//!
//! When the timer fires it pauses through the normal command channel, the
//! same as pressing Pause: the position is kept and Play resumes there (a
//! radio station restarts its stream). If playback is already paused that
//! pause changes nothing and the timer just clears. Pausing manually does
//! not cancel a minutes timer, but it does end an end-of-track timer (the
//! stop counts as the end of the track).
//!
//! After the pause, the settings (`sleep_timer_settings`, read when the
//! timer fires) can add two steps, in this order: power off through the
//! firmware control channel, then shut down the system. Both send the same
//! `SystemCommand` as the buttons in Settings, so the shutdown takes the
//! usual path (poweroff, SIGTERM, database persisted). They wait for the
//! pause to land first, and are skipped (logged) where the matching button
//! is not offered: firmware power without the USB command channel, shutdown
//! in demo or desktop/Android mode or off Linux. They also run when a
//! minutes timer finds playback already paused: the listener asked for the
//! device to go off at that time.
//!
//! As a multiroom leader the pause stops the whole group; as a grouped
//! follower local transport is locked, so the timer cannot be set there and
//! one that fires after joining a group only clears itself.
//!
//! The timer lives in memory only: restarting `RSPlayer` clears it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use api_models::common::{PlayerCommand, SystemCommand, UserCommand};
use api_models::playback_source::PlaybackSource;
use api_models::settings::Settings;
use api_models::sleep_timer::{SleepTimerRequest, SleepTimerState, SLEEP_TIMER_MAX_MINUTES};
use api_models::state::{PlayerState, StateChangeEvent};
use log::{info, warn};
use playback::rsp::player_service::PlayerService;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc;
use tokio::time::Instant;

/// How long a queued pause may take to land before the extra steps run
/// anyway (the command handler waits up to 3 s for a playback thread).
const PAUSE_WAIT: Duration = Duration::from_secs(5);
/// Lets the pause reach the command queue and its STOPPED be persisted.
const PAUSE_SETTLE: Duration = Duration::from_secs(1);

const FOLLOWER_ERROR: &str = "Sleep timer: playback is controlled by the multiroom group leader.";

/// The song the player is on, as far as the sleep timer cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTrack {
    pub file: String,
    /// A live stream with no end (radio).
    pub endless: bool,
}

/// What the sleep timer needs from the player; a fake in tests.
pub trait PlayerControl: Send + Sync {
    fn is_playing(&self) -> bool;
    fn current_track(&self) -> Option<CurrentTrack>;
    /// True while this instance plays as a grouped multiroom follower.
    fn is_follower(&self) -> bool;
    fn pause(&self);
    /// The configured extra steps that are available right now.
    fn after_pause(&self) -> AfterPause;
    /// Sends these, in order, to the system command handler.
    fn system_commands(&self, cmds: Vec<SystemCommand>);
}

/// Steps after the pause, from `sleep_timer_settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AfterPause {
    pub firmware_power_off: bool,
    pub shutdown_system: bool,
}

impl AfterPause {
    const fn any(self) -> bool {
        self.firmware_power_off || self.shutdown_system
    }

    const fn describe(self) -> &'static str {
        match (self.firmware_power_off, self.shutdown_system) {
            (true, true) => ", powering off and shutting down",
            (true, false) => ", powering off",
            (false, true) => ", shutting down",
            (false, false) => "",
        }
    }
}

enum Msg {
    Set(SleepTimerRequest),
    Query,
}

/// Sends requests to the sleep timer task; cheap to clone.
#[derive(Clone)]
pub struct SleepTimerHandle {
    tx: mpsc::UnboundedSender<Msg>,
}

impl SleepTimerHandle {
    pub fn set(&self, request: SleepTimerRequest) {
        self.send(Msg::Set(request));
    }

    /// Broadcasts the current state.
    pub fn query(&self) {
        self.send(Msg::Query);
    }

    fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_err() {
            warn!("Sleep timer task is not running");
        }
    }
}

/// The receiving end, consumed by [`run`].
pub struct SleepTimerReceiver(mpsc::UnboundedReceiver<Msg>);

#[must_use]
pub fn channel() -> (SleepTimerHandle, SleepTimerReceiver) {
    let (tx, rx) = mpsc::unbounded_channel();
    (SleepTimerHandle { tx }, SleepTimerReceiver(rx))
}

enum Armed {
    Off,
    Until {
        deadline: Instant,
        state: SleepTimerState,
    },
    EndOfTrack {
        file: String,
        /// Subscribed when armed, so older events cannot fire it.
        events: broadcast::Receiver<StateChangeEvent>,
    },
}

impl Armed {
    const fn state(&self) -> SleepTimerState {
        match self {
            Self::Off => SleepTimerState::Off,
            Self::Until { state, .. } => *state,
            Self::EndOfTrack { .. } => SleepTimerState::EndOfTrack,
        }
    }
}

enum Wake {
    Msg(Msg),
    Fire,
    Nothing,
    Shutdown,
}

/// Runs the sleep timer until every [`SleepTimerHandle`] is dropped.
pub async fn run(control: Arc<dyn PlayerControl>, rx: SleepTimerReceiver, events_tx: broadcast::Sender<StateChangeEvent>) {
    let mut rx = rx.0;
    let mut armed = Armed::Off;
    loop {
        match next_wake(&mut armed, &mut rx, control.as_ref()).await {
            Wake::Msg(Msg::Query) => send(&events_tx, StateChangeEvent::SleepTimerEvent(armed.state())),
            Wake::Msg(Msg::Set(request)) => match arm(request, control.as_ref(), &events_tx) {
                Ok(new) => {
                    armed = new;
                    info!("Sleep timer set: {:?}", armed.state());
                    send(&events_tx, StateChangeEvent::SleepTimerEvent(armed.state()));
                }
                Err(message) => send(&events_tx, StateChangeEvent::NotificationError(message)),
            },
            Wake::Fire => {
                let end_of_track = matches!(armed, Armed::EndOfTrack { .. });
                armed = Armed::Off;
                fire(control.as_ref(), &events_tx, end_of_track).await;
            }
            Wake::Nothing => {}
            Wake::Shutdown => return,
        }
    }
}

async fn next_wake(armed: &mut Armed, rx: &mut mpsc::UnboundedReceiver<Msg>, control: &dyn PlayerControl) -> Wake {
    let msg = |m: Option<Msg>| m.map_or(Wake::Shutdown, Wake::Msg);
    match armed {
        Armed::Off => msg(rx.recv().await),
        Armed::Until { deadline, .. } => {
            let deadline = *deadline;
            tokio::select! {
                m = rx.recv() => msg(m),
                () = tokio::time::sleep_until(deadline) => Wake::Fire,
            }
        }
        Armed::EndOfTrack { file, events } => {
            tokio::select! {
                m = rx.recv() => msg(m),
                ev = events.recv() => match ev {
                    Err(RecvError::Closed) => Wake::Shutdown,
                    ev => if track_ended(ev, file, control) { Wake::Fire } else { Wake::Nothing },
                },
            }
        }
    }
}

fn track_ended(event: Result<StateChangeEvent, RecvError>, file: &str, control: &dyn PlayerControl) -> bool {
    match event {
        Ok(StateChangeEvent::CurrentSongEvent(song)) => song.file != file,
        Ok(StateChangeEvent::PlaybackStateEvent(state)) => !matches!(state, PlayerState::PLAYING),
        Ok(_) => false,
        // Missed events: look at the player instead.
        Err(_) => !control.is_playing() || control.current_track().is_none_or(|t| t.file != file),
    }
}

fn arm(
    request: SleepTimerRequest,
    control: &dyn PlayerControl,
    events_tx: &broadcast::Sender<StateChangeEvent>,
) -> Result<Armed, String> {
    if request != SleepTimerRequest::Off && control.is_follower() {
        return Err(FOLLOWER_ERROR.to_string());
    }
    match request {
        SleepTimerRequest::Off => Ok(Armed::Off),
        SleepTimerRequest::Minutes(minutes) => {
            if minutes == 0 || minutes > SLEEP_TIMER_MAX_MINUTES {
                return Err(format!("Sleep timer: choose between 1 and {SLEEP_TIMER_MAX_MINUTES} minutes."));
            }
            Ok(Armed::Until {
                deadline: Instant::now() + Duration::from_secs(u64::from(minutes) * 60),
                state: SleepTimerState::minutes_from_now(minutes),
            })
        }
        SleepTimerRequest::EndOfTrack => {
            let track = control
                .current_track()
                .filter(|_| control.is_playing())
                .ok_or_else(|| "Sleep timer: nothing is playing.".to_string())?;
            if track.endless {
                return Err("Sleep timer: a live stream has no track end, choose a number of minutes instead.".to_string());
            }
            Ok(Armed::EndOfTrack {
                file: track.file,
                events: events_tx.subscribe(),
            })
        }
    }
}

async fn fire(control: &dyn PlayerControl, events_tx: &broadcast::Sender<StateChangeEvent>, end_of_track: bool) {
    if control.is_follower() {
        send(events_tx, StateChangeEvent::SleepTimerEvent(SleepTimerState::Off));
        send(events_tx, StateChangeEvent::NotificationError(FOLLOWER_ERROR.to_string()));
        return;
    }
    // Pause even when the player looks idle: the event that fired an
    // end-of-track timer may be the STOPPED of a Next that is about to
    // start the following song. The pause is queued behind that command,
    // and pausing an idle player does nothing.
    let was_playing = end_of_track || control.is_playing();
    let after = control.after_pause();
    info!("Sleep timer fired, pausing playback ({after:?})");
    control.pause();
    let what = if was_playing { "Sleep timer: playback paused" } else { "Sleep timer ended" };
    send(events_tx, StateChangeEvent::SleepTimerEvent(SleepTimerState::Off));
    send(events_tx, StateChangeEvent::NotificationSuccess(format!("{what}{}", after.describe())));
    if !after.any() {
        return;
    }
    // The pause goes through the user command queue, the extra steps
    // through the system one: wait until it has landed.
    tokio::time::sleep(PAUSE_SETTLE).await;
    let give_up = Instant::now() + PAUSE_WAIT;
    while control.is_playing() && Instant::now() < give_up {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(PAUSE_SETTLE).await;
    let mut cmds = Vec::new();
    if after.firmware_power_off {
        info!("Sleep timer: powering off through the firmware control channel");
        cmds.push(SystemCommand::SetFirmwarePower(false));
    }
    if after.shutdown_system {
        info!("Sleep timer: shutting down the system");
        cmds.push(SystemCommand::PowerOff);
    }
    control.system_commands(cmds);
}

fn send(events_tx: &broadcast::Sender<StateChangeEvent>, event: StateChangeEvent) {
    let _ = events_tx.send(event);
}

/// The configured extra steps, minus those whose Settings button is not
/// offered. `linux`: the shutdown handler only acts there.
fn available_after_pause(settings: &Settings, linux: bool) -> AfterPause {
    let wanted = settings.sleep_timer_settings;
    let firmware = settings.firmware_power_available();
    let shutdown = settings.system_power_available() && linux;
    if wanted.firmware_power_off && !firmware {
        info!("Sleep timer: firmware power off skipped, the USB command channel is not available");
    }
    if wanted.shutdown_system && !shutdown {
        info!("Sleep timer: shutdown skipped, not available in this mode or on this platform");
    }
    AfterPause {
        firmware_power_off: wanted.firmware_power_off && firmware,
        shutdown_system: wanted.shutdown_system && shutdown,
    }
}

/// [`PlayerControl`] backed by the real player. Pausing goes through the
/// user command channel, so it is ordered with the listener's own commands.
pub struct ServerPlayerControl {
    pub player_service: Arc<PlayerService>,
    pub follower_active: Arc<AtomicBool>,
    pub commands_tx: mpsc::Sender<UserCommand>,
    pub system_commands_tx: mpsc::Sender<SystemCommand>,
    pub config: config::ArcConfiguration,
}

impl PlayerControl for ServerPlayerControl {
    fn is_playing(&self) -> bool {
        self.player_service.get_player_state() == PlayerState::PLAYING
    }

    fn current_track(&self) -> Option<CurrentTrack> {
        let song = self
            .player_service
            .get_current_song()
            .or_else(|| self.player_service.current_source_song())?;
        let endless = match self.player_service.playback_source() {
            PlaybackSource::Radio(_) => true,
            PlaybackSource::Podcast { .. } => false,
            // A stream URL in the queue without a duration is a station too.
            PlaybackSource::Queue => {
                (song.file.starts_with("http://") || song.file.starts_with("https://")) && song.time.is_none()
            }
        };
        Some(CurrentTrack { file: song.file, endless })
    }

    fn is_follower(&self) -> bool {
        self.follower_active.load(Ordering::SeqCst)
    }

    fn after_pause(&self) -> AfterPause {
        let mut settings = self.config.get_settings();
        // As the settings API reports them to the UI.
        settings.demo_mode = std::env::var("DEMO_MODE").is_ok();
        settings.desktop_mode = std::env::var("RSPLAYER_DESKTOP").is_ok();
        available_after_pause(&settings, cfg!(target_os = "linux"))
    }

    fn system_commands(&self, cmds: Vec<SystemCommand>) {
        let tx = self.system_commands_tx.clone();
        tokio::spawn(async move {
            for cmd in cmds {
                if let Err(e) = tx.send(cmd).await {
                    warn!("Sleep timer could not send a system command: {e}");
                }
            }
        });
    }

    fn pause(&self) {
        let tx = self.commands_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = tx.send(UserCommand::Player(PlayerCommand::Pause)).await {
                warn!("Sleep timer could not pause playback: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_models::player::Song;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakePlayer {
        playing: AtomicBool,
        follower: AtomicBool,
        track: Mutex<Option<CurrentTrack>>,
        pauses: std::sync::atomic::AtomicUsize,
        after: Mutex<AfterPause>,
        /// The pause only lands when the test says so.
        slow_pause: AtomicBool,
        /// "pause" and system commands, in the order they were sent.
        log: Mutex<Vec<String>>,
    }

    impl FakePlayer {
        fn playing(file: &str, endless: bool) -> Arc<Self> {
            let p = Self::default();
            p.playing.store(true, Ordering::SeqCst);
            *p.track.lock().unwrap() = Some(CurrentTrack {
                file: file.to_string(),
                endless,
            });
            Arc::new(p)
        }
        fn pauses(&self) -> usize {
            self.pauses.load(Ordering::SeqCst)
        }
        fn with_after(self: Arc<Self>, firmware_power_off: bool, shutdown_system: bool) -> Arc<Self> {
            *self.after.lock().unwrap() = AfterPause {
                firmware_power_off,
                shutdown_system,
            };
            self
        }
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl PlayerControl for FakePlayer {
        fn is_playing(&self) -> bool {
            self.playing.load(Ordering::SeqCst)
        }
        fn current_track(&self) -> Option<CurrentTrack> {
            self.track.lock().unwrap().clone()
        }
        fn is_follower(&self) -> bool {
            self.follower.load(Ordering::SeqCst)
        }
        fn pause(&self) {
            if !self.slow_pause.load(Ordering::SeqCst) {
                self.playing.store(false, Ordering::SeqCst);
            }
            self.pauses.fetch_add(1, Ordering::SeqCst);
            self.log.lock().unwrap().push("pause".to_string());
        }
        fn after_pause(&self) -> AfterPause {
            *self.after.lock().unwrap()
        }
        fn system_commands(&self, cmds: Vec<SystemCommand>) {
            self.log.lock().unwrap().extend(cmds.iter().map(|c| format!("{c:?}")));
        }
    }

    struct Harness {
        player: Arc<FakePlayer>,
        timer: SleepTimerHandle,
        events_tx: broadcast::Sender<StateChangeEvent>,
        events: broadcast::Receiver<StateChangeEvent>,
    }

    impl Harness {
        fn new(player: Arc<FakePlayer>) -> Self {
            let (events_tx, events) = broadcast::channel(64);
            let (timer, rx) = channel();
            let control: Arc<dyn PlayerControl> = player.clone();
            tokio::spawn(run(control, rx, events_tx.clone()));
            Self {
                player,
                timer,
                events_tx,
                events,
            }
        }

        /// Lets the timer task run, then returns the sleep timer and
        /// notification events it sent, in order.
        async fn drain(&mut self) -> Vec<StateChangeEvent> {
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            let mut out = Vec::new();
            while let Ok(ev) = self.events.try_recv() {
                if matches!(
                    ev,
                    StateChangeEvent::SleepTimerEvent(_)
                        | StateChangeEvent::NotificationSuccess(_)
                        | StateChangeEvent::NotificationError(_)
                ) {
                    out.push(ev);
                }
            }
            out
        }
    }

    fn song(file: &str) -> Song {
        Song {
            file: file.to_string(),
            ..Song::default()
        }
    }

    const PAUSED: &str = "Sleep timer: playback paused";

    fn paused_events() -> Vec<StateChangeEvent> {
        vec![
            StateChangeEvent::SleepTimerEvent(SleepTimerState::Off),
            StateChangeEvent::NotificationSuccess(PAUSED.to_string()),
        ]
    }

    #[tokio::test(start_paused = true)]
    async fn fires_after_minutes() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(30));
        let events = h.drain().await;
        assert!(matches!(events.as_slice(), [StateChangeEvent::SleepTimerEvent(SleepTimerState::Until { .. })]));

        tokio::time::advance(Duration::from_secs(29 * 60 + 59)).await;
        assert!(h.drain().await.is_empty());
        assert_eq!(h.player.pauses(), 0);

        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(h.drain().await, paused_events());
        assert_eq!(h.player.pauses(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn new_timer_replaces_old() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(10));
        h.drain().await;
        tokio::time::advance(Duration::from_secs(5 * 60)).await;
        h.timer.set(SleepTimerRequest::Minutes(15));
        h.drain().await;

        // The first deadline passes without effect.
        tokio::time::advance(Duration::from_secs(10 * 60)).await;
        assert!(h.drain().await.is_empty());
        assert_eq!(h.player.pauses(), 0);

        tokio::time::advance(Duration::from_secs(5 * 60)).await;
        assert_eq!(h.drain().await, paused_events());
        assert_eq!(h.player.pauses(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn off_cancels() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        h.timer.set(SleepTimerRequest::Off);
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::Off)]);

        tokio::time::advance(Duration::from_secs(120)).await;
        assert!(h.drain().await.is_empty());
        assert_eq!(h.player.pauses(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn query_reports_state() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.query();
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::Off)]);
        h.timer.set(SleepTimerRequest::EndOfTrack);
        h.drain().await;
        h.timer.query();
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::EndOfTrack)]);
    }

    #[tokio::test(start_paused = true)]
    async fn clears_quietly_when_already_paused() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        h.player.playing.store(false, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(
            h.drain().await,
            vec![
                StateChangeEvent::SleepTimerEvent(SleepTimerState::Off),
                StateChangeEvent::NotificationSuccess("Sleep timer ended".to_string()),
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn end_of_track_fires_on_song_change() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::EndOfTrack);
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::EndOfTrack)]);

        // A client reconnecting re-sends the same song: no track change.
        h.events_tx.send(StateChangeEvent::CurrentSongEvent(song("a.flac"))).unwrap();
        h.drain().await;
        assert_eq!(h.player.pauses(), 0);

        h.events_tx.send(StateChangeEvent::CurrentSongEvent(song("b.flac"))).unwrap();
        assert_eq!(h.drain().await, paused_events());
        assert_eq!(h.player.pauses(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn end_of_track_fires_when_playback_stops() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::EndOfTrack);
        h.drain().await;
        // Next stops the old song before starting the new one: the timer
        // sees STOPPED with the player idle and still pauses, so the pause
        // lands after the new song started.
        h.player.playing.store(false, Ordering::SeqCst);
        h.events_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).unwrap();
        let events = h.drain().await;
        assert_eq!(events[0], StateChangeEvent::SleepTimerEvent(SleepTimerState::Off));
        assert_eq!(h.player.pauses(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn end_of_track_rejected_for_radio() {
        let mut h = Harness::new(FakePlayer::playing("http://radio/stream", true));
        h.timer.set(SleepTimerRequest::EndOfTrack);
        let events = h.drain().await;
        assert!(matches!(events.as_slice(), [StateChangeEvent::NotificationError(_)]));
        h.timer.query();
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::Off)]);
    }

    #[tokio::test(start_paused = true)]
    async fn rejects_bad_minutes_and_followers() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(0));
        h.timer.set(SleepTimerRequest::Minutes(SLEEP_TIMER_MAX_MINUTES + 1));
        h.player.follower.store(true, Ordering::SeqCst);
        h.timer.set(SleepTimerRequest::Minutes(10));
        let events = h.drain().await;
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|e| matches!(e, StateChangeEvent::NotificationError(_))));
        // Turning it off is always allowed.
        h.timer.set(SleepTimerRequest::Off);
        assert_eq!(h.drain().await, vec![StateChangeEvent::SleepTimerEvent(SleepTimerState::Off)]);
    }

    const FIRMWARE_OFF: &str = "SetFirmwarePower(false)";
    const POWER_OFF: &str = "PowerOff";

    /// Lets a fired timer finish its extra steps.
    async fn settle(h: &mut Harness) -> Vec<StateChangeEvent> {
        let events = h.drain().await;
        tokio::time::sleep(Duration::from_secs(10)).await;
        events
    }

    #[tokio::test(start_paused = true)]
    async fn default_only_pauses() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false));
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(settle(&mut h).await, paused_events());
        assert_eq!(h.player.log(), vec!["pause"]);
    }

    #[tokio::test(start_paused = true)]
    async fn firmware_power_off_after_pause() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false).with_after(true, false));
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        let events = h.drain().await;
        assert_eq!(
            events[1],
            StateChangeEvent::NotificationSuccess("Sleep timer: playback paused, powering off".to_string())
        );
        // The pause comes first; the power off waits for it to land.
        assert_eq!(h.player.log(), vec!["pause"]);
        settle(&mut h).await;
        assert_eq!(h.player.log(), vec!["pause", FIRMWARE_OFF]);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_after_pause() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false).with_after(false, true));
        h.timer.set(SleepTimerRequest::EndOfTrack);
        h.drain().await;
        h.events_tx.send(StateChangeEvent::CurrentSongEvent(song("b.flac"))).unwrap();
        let events = settle(&mut h).await;
        assert_eq!(
            events[1],
            StateChangeEvent::NotificationSuccess("Sleep timer: playback paused, shutting down".to_string())
        );
        assert_eq!(h.player.log(), vec!["pause", POWER_OFF]);
    }

    #[tokio::test(start_paused = true)]
    async fn both_steps_in_order() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false).with_after(true, true));
        h.timer.set(SleepTimerRequest::Minutes(5));
        h.drain().await;
        tokio::time::advance(Duration::from_secs(300)).await;
        let events = settle(&mut h).await;
        assert_eq!(
            events[1],
            StateChangeEvent::NotificationSuccess("Sleep timer: playback paused, powering off and shutting down".to_string())
        );
        assert_eq!(h.player.log(), vec!["pause", FIRMWARE_OFF, POWER_OFF]);
    }

    #[tokio::test(start_paused = true)]
    async fn steps_wait_for_the_pause_to_land() {
        let player = FakePlayer::playing("a.flac", false).with_after(false, true);
        player.slow_pause.store(true, Ordering::SeqCst);
        let mut h = Harness::new(player);
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        h.drain().await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(h.player.log(), vec!["pause"], "still playing: no shutdown yet");
        h.player.playing.store(false, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(h.player.log(), vec!["pause", POWER_OFF]);
    }

    #[tokio::test(start_paused = true)]
    async fn steps_run_when_already_paused() {
        let mut h = Harness::new(FakePlayer::playing("a.flac", false).with_after(false, true));
        h.timer.set(SleepTimerRequest::Minutes(1));
        h.drain().await;
        h.player.playing.store(false, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(60)).await;
        let events = settle(&mut h).await;
        assert_eq!(
            events[1],
            StateChangeEvent::NotificationSuccess("Sleep timer ended, shutting down".to_string())
        );
        assert_eq!(h.player.log(), vec!["pause", POWER_OFF]);
    }

    #[test]
    fn steps_skipped_where_the_buttons_are_not_offered() {
        let mut settings = Settings::default();
        settings.sleep_timer_settings.firmware_power_off = true;
        settings.sleep_timer_settings.shutdown_system = true;
        // USB command channel off: no firmware power off.
        assert_eq!(
            available_after_pause(&settings, true),
            AfterPause {
                firmware_power_off: false,
                shutdown_system: true
            }
        );
        settings.usb_settings.enabled = true;
        assert_eq!(
            available_after_pause(&settings, true),
            AfterPause {
                firmware_power_off: true,
                shutdown_system: true
            }
        );
        // The PowerOff handler only acts on Linux.
        assert!(!available_after_pause(&settings, false).shutdown_system);
        for mode in [|s: &mut Settings| s.demo_mode = true, |s: &mut Settings| s.desktop_mode = true] {
            let mut s = settings.clone();
            mode(&mut s);
            assert_eq!(available_after_pause(&s, true), AfterPause::default());
        }
        // Nothing configured: nothing runs.
        assert_eq!(available_after_pause(&Settings::default(), true), AfterPause::default());
    }
}
