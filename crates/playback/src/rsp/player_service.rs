//! Playback orchestration: the boundary between async command handling and
//! the blocking audio world.
//!
//! [`PlayerService`] owns the playback thread (spawned per play/resume at
//! the configured `player_threads_priority` — never `Min`, which starved
//! audio on single-core devices), advances the queue on `SongFinished`,
//! persists pause/progress state in the `player_state` keyspace for resume
//! across restarts, pauses the loudness scan while playing, and owns the
//! `DspProcessor` (settings changes rebuild the EQ from here). Control in:
//! atomics (`stop_signal`, `skip_to_time`); results out:
//! `StateChangeEvent`s.
//!
//! Before each track starts, an optional [`ResumePositionProvider`] (the
//! podcast service) can arm `skip_to_time` so episodes continue where the
//! listener left off, whatever path (play, next, queue click) started them.

use fjall::{Database, Keyspace};
use log::{debug, error, info, trace, warn};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, Ordering},
};
use std::thread::JoinHandle;
use thread_priority::{ThreadBuilder, ThreadPriority};
use tokio::sync::broadcast::{Sender, error::RecvError};

use api_models::{
    player::Song,
    settings::{DspSettings, RsPlayerSettings, Settings},
    state::{PlayerInfo, PlayerState, StateChangeEvent},
};
use metadata::loudness_service::LoudnessService;
use metadata::metadata_service::MetadataService;
use metadata::queue_service::QueueService;

use dsp::DspProcessor;

use super::audio_source::is_http_stream;
use super::symphonia::{OutputUnavailable, PlaybackResult};
use crate::rsp::playback_config::PlaybackConfig;
use crate::rsp::playback_context::PlaybackContext;
use crate::rsp::tee::SyncTee;

/// Supplies a per-song start offset (seconds) consulted right before a
/// track begins playing; `None` means start from the beginning.
pub trait ResumePositionProvider: Send + Sync {
    fn resume_position(&self, song: &Song) -> Option<u16>;
}

pub struct PlayerService {
    state_db: Keyspace,
    queue_service: Arc<QueueService>,
    metadata_service: Arc<MetadataService>,
    playback_thread_handle: Arc<Mutex<Option<JoinHandle<PlaybackResult>>>>,
    stop_signal: Arc<AtomicBool>,
    /// Set when the playback thread ran off the end of the queue: the
    /// STOPPED it sends marks the song paused, but there is nothing to resume.
    queue_finished: Arc<AtomicBool>,
    skip_to_time: Arc<AtomicU16>,
    last_known_time: Arc<AtomicU32>,
    current_volume: Arc<AtomicU8>,
    software_gain_active: bool,
    audio_device: String,
    rsp_settings: RsPlayerSettings,
    local_browser_playback: bool,
    changes_tx: Sender<StateChangeEvent>,
    dsp_processor: Arc<Mutex<Option<DspProcessor>>>,
    loudness_service: Arc<LoudnessService>,
    last_player_info: Arc<Mutex<Option<PlayerInfo>>>,
    /// Last broadcast song, including in-stream (ICY) radio titles that
    /// never reach the queue.
    last_song: Arc<Mutex<Option<Song>>>,
    sync_tee: Option<SyncTee>,
    resume_provider: Option<Arc<dyn ResumePositionProvider>>,
}

const LAST_SONG_PAUSED_KEY: &str = "last_song_paused";
const LAST_SONG_PROGRESS_KEY: &str = "last_played_song_progress";

impl PlayerService {
    #[must_use]
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn new(
        db: &Database,
        settings: &Settings,
        current_volume: Arc<AtomicU8>,
        metadata_service: Arc<MetadataService>,
        queue_service: Arc<QueueService>,
        state_changes_tx: Sender<StateChangeEvent>,
        loudness_service: Arc<LoudnessService>,
        sync_tee: Option<SyncTee>,
        resume_provider: Option<Arc<dyn ResumePositionProvider>>,
    ) -> Arc<Self> {
        let state_db = db
            .keyspace("player_state", config::db_maintenance::small_memtable_options)
            .expect("Failed to open player_state keyspace");
        let state_db_async = state_db.clone();
        let mut rx = state_changes_tx.subscribe();
        let state_tx = state_changes_tx.clone();

        let initial_time = Self::read_last_played_song_time(&state_db);
        let last_known_time = Arc::new(AtomicU32::new(initial_time));
        let last_known_time_clone = last_known_time.clone();
        let current_volume_clone = current_volume.clone();
        let software_gain_active = settings.volume_ctrl_settings.ctrl_device == api_models::common::VolumeCrtlType::Software;
        let dsp_processor = Arc::new(Mutex::new({
            let rsp = &settings.rs_player_settings;
            if rsp.dsp_settings.enabled || rsp.loudness_normalization_enabled {
                let effective_dsp = if rsp.dsp_settings.enabled {
                    rsp.dsp_settings.clone()
                } else {
                    DspSettings {
                        enabled: false,
                        filters: vec![],
                    }
                };
                Some(DspProcessor::new(effective_dsp))
            } else {
                None
            }
        }));
        let dsp_processor_clone = dsp_processor.clone();
        let last_player_info = Arc::new(Mutex::new(None));
        let last_player_info_clone = last_player_info.clone();
        let last_song = Arc::new(Mutex::new(None));
        let last_song_clone = last_song.clone();

        tokio::task::spawn(async move {
            let mut last_saved_secs: u64 = u64::MAX;
            loop {
                let event = match rx.recv().await {
                    Ok(event) => event,
                    Err(RecvError::Lagged(skipped)) => {
                        warn!("State change receiver lagged, {skipped} events skipped");
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                };
                match event {
                    StateChangeEvent::SongTimeEvent(st) => {
                        let lt_secs = st.current_time.as_secs();
                        #[allow(clippy::cast_possible_truncation)]
                        last_known_time_clone.store(lt_secs as u32, Ordering::Relaxed);
                        if lt_secs != last_saved_secs {
                            last_saved_secs = lt_secs;
                            let lt = lt_secs.to_string();
                            trace!("Save time state: {lt}");
                            _ = state_db_async.insert(LAST_SONG_PROGRESS_KEY, lt.as_bytes());
                        }
                    }
                    StateChangeEvent::PlaybackStateEvent(ps) => {
                        debug!("Save player state: {ps:?}");
                        match ps {
                            PlayerState::PLAYING => {
                                _ = state_db_async.remove(LAST_SONG_PAUSED_KEY);
                            }
                            PlayerState::PAUSED | PlayerState::STOPPED => {
                                _ = state_db_async.insert(LAST_SONG_PAUSED_KEY, "true");
                            }
                            PlayerState::ERROR(msg) => {
                                state_tx
                                    .send(StateChangeEvent::NotificationError(format!("Failed to play {msg}")))
                                    .ok();
                            }
                        }
                    }
                    StateChangeEvent::VolumeChangeEvent(vol) => {
                        current_volume_clone.store(vol.current, Ordering::Relaxed);
                    }
                    StateChangeEvent::CurrentSongEvent(song) => {
                        if let Ok(mut guard) = last_song_clone.lock() {
                            *guard = Some(song);
                        }
                    }
                    StateChangeEvent::PlayerInfoEvent(info) => {
                        if let Ok(mut guard) = last_player_info_clone.lock() {
                            *guard = Some(info.clone());
                        }
                        if let Ok(mut guard) = dsp_processor_clone.lock()
                            && let Some(proc) = guard.as_mut()
                        {
                            if let Some(rate) = info.audio_format_rate {
                                proc.rate = rate as usize;
                            }
                            if let Some(channels) = info.audio_format_channels {
                                proc.channels = channels;
                            }
                            info!("DSP processor updated with rate: {}, channels: {}", proc.rate, proc.channels);
                        }
                    }
                    _ => (),
                }
            }
        });

        let ps = PlayerService {
            state_db,
            changes_tx: state_changes_tx,
            queue_service,
            metadata_service,
            playback_thread_handle: Arc::new(Mutex::new(None)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            queue_finished: Arc::new(AtomicBool::new(false)),
            skip_to_time: Arc::new(AtomicU16::new(0)),
            last_known_time,
            current_volume,
            software_gain_active,
            audio_device: settings.alsa_settings.output_device.name.clone(),
            rsp_settings: settings.rs_player_settings.clone(),
            local_browser_playback: settings.local_browser_playback,
            dsp_processor,
            loudness_service,
            last_player_info,
            last_song,
            sync_tee,
            resume_provider,
        };
        let last_played_song_progress = ps.get_last_played_song_time();
        if last_played_song_progress > 0 {
            ps.seek_current_song(last_played_song_progress);
        }
        Arc::new(ps)
    }

    /// Shared DSP handle so the multiroom sink applies this room's EQ.
    #[must_use]
    pub fn dsp_handle(&self) -> Option<dsp::DspHandle> {
        self.dsp_processor.lock().ok().and_then(|g| g.as_ref().map(DspProcessor::handle))
    }

    pub fn play_from_current_queue_song(&self) {
        if self.is_playing() {
            return;
        }
        let queue_finished = self.queue_finished.swap(false, Ordering::Relaxed);
        if !queue_finished && let Ok(Some(_)) = self.state_db.get(LAST_SONG_PAUSED_KEY) {
            let last_song_time = self.get_last_played_song_time();
            self.seek_current_song(last_song_time);
        }

        *self.playback_thread_handle.lock().expect("lock poisoned") = Some(self.play_all_in_queue());
    }

    fn is_playing(&self) -> bool {
        self.playback_thread_handle
            .lock()
            .expect("lock poisoned")
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    /// Info of the song currently being decoded, if playback is active (playing or paused).
    pub fn get_current_player_info(&self) -> Option<PlayerInfo> {
        if !self.is_playing() {
            return None;
        }
        self.last_player_info.lock().ok().and_then(|guard| guard.clone())
    }

    /// Actual transport state, for clients that missed the
    /// `PlaybackStateEvent`s (reconnect, lagging websocket).
    pub fn get_player_state(&self) -> PlayerState {
        if self.is_playing() { PlayerState::PLAYING } else { PlayerState::STOPPED }
    }

    /// Song as last broadcast to clients, if playback is active (playing or
    /// paused). Unlike the queue entry, this carries the live stream title
    /// of a radio station.
    pub fn get_current_song(&self) -> Option<Song> {
        if !self.is_playing() {
            return None;
        }
        self.last_song.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn play_from_beginning(&self) {
        self.stop_current_song();
        _ = self.state_db.remove(LAST_SONG_PAUSED_KEY);
        *self.playback_thread_handle.lock().expect("lock poisoned") = Some(self.play_all_in_queue());
    }

    pub fn play_next_song(&self) {
        self.stop_current_song();
        self.queue_service.move_current_to_next_song();
        _ = self.state_db.remove(LAST_SONG_PAUSED_KEY);
        self.play_from_current_queue_song();
    }

    pub fn play_prev_song(&self) {
        self.stop_current_song();
        self.queue_service.move_current_to_previous_song();
        _ = self.state_db.remove(LAST_SONG_PAUSED_KEY);
        self.play_from_current_queue_song();
    }

    pub fn stop_current_song(&self) -> Option<PlaybackResult> {
        self.stop_signal.store(true, Ordering::Relaxed);
        let handle = self.playback_thread_handle.lock().expect("lock poisoned").take();
        handle.and_then(|h| h.join().ok())
    }

    pub fn toggle_play_pause(&self) {
        if self.is_playing() {
            self.stop_current_song();
        } else {
            self.play_from_current_queue_song();
        }
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    pub fn seek_current_song(&self, seconds: u16) {
        self.skip_to_time.store(seconds, Ordering::Relaxed);
    }

    pub fn seek_relative(&self, offset_seconds: i32) {
        #[allow(clippy::cast_possible_wrap)]
        let current = self.last_known_time.load(Ordering::Relaxed) as i32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let new_time = (current + offset_seconds).max(0) as u16;
        self.seek_current_song(new_time);
    }

    pub fn play_song(&self, song_id: &str) {
        self.stop_current_song();
        self.queue_service.move_current_to(song_id);
        _ = self.state_db.remove(LAST_SONG_PAUSED_KEY);
        self.play_from_current_queue_song();
    }

    pub fn update_dsp_settings(&self, dsp_settings: &DspSettings) {
        if let Ok(mut guard) = self.dsp_processor.lock() {
            if let Some(dsp) = &mut *guard {
                dsp.update_settings(dsp_settings);
            } else {
                debug!("DSP update requested but DSP is disabled");
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn play_all_in_queue(&self) -> JoinHandle<PlaybackResult> {
        self.stop_signal.store(false, Ordering::Relaxed);
        self.queue_finished.store(false, Ordering::Relaxed);
        let stop_signal = self.stop_signal.clone();
        let queue_finished = self.queue_finished.clone();
        let skip_to_time = self.skip_to_time.clone();
        let queue = self.queue_service.clone();
        let audio_device = self.audio_device.clone();
        let playback_thread_prio = self.rsp_settings.player_threads_priority;
        let music_dirs = self.metadata_service.effective_directories();
        let changes_tx = self.changes_tx.clone();
        let rsp_settings = self.rsp_settings.clone();
        let metadata_service = self.metadata_service.clone();
        let dsp_handle = self.dsp_processor.lock().ok().and_then(|g| g.as_ref().map(DspProcessor::handle));
        let current_volume = self.current_volume.clone();
        let software_gain = if self.software_gain_active { Some(current_volume) } else { None };
        let vu_meter_enabled = self.rsp_settings.vu_meter_enabled;
        let loudness_service = self.loudness_service.clone();
        let is_multi_core_platform = core_affinity::get_core_ids().is_some_and(|ids| ids.len() > 1);
        let local_browser_playback = self.local_browser_playback;
        let sync_tee = self.sync_tee.clone();
        let resume_provider = self.resume_provider.clone();
        // Use the configured priority on single-core platforms too: with
        // ThreadPriority::Min the audio thread on an RPi Zero was starved by
        // web-UI/library requests sharing the one core, breaking playback.
        let prio = ThreadPriority::Crossplatform(playback_thread_prio.try_into().expect("invalid thread priority value"));
        ThreadBuilder::default()
            .name("playback".to_string())
            .priority(prio)
            .spawn(move |prio| {
                // Only network streams are retried: a dropped connection or a
                // station restarting can recover, a local file or audio output
                // failure fails the same way again.
                const MAX_STREAM_RETRIES: u32 = 3;
                // A stream that played this long before failing starts over
                // with a full set of retries (radio never "finishes").
                const STREAM_RETRY_RESET: std::time::Duration = std::time::Duration::from_secs(30);
                // A song that fails for good is skipped, but this many failures
                // in a row (library mount gone, Loop Single on a broken file)
                // stop playback instead of running through the whole queue.
                const MAX_CONSECUTIVE_FAILURES: u32 = 3;
                loudness_service.set_playback_active(true);
                if prio.is_ok() {
                    info!("Playback thread started with priority {playback_thread_prio:?}");
                } else {
                    warn!("Failed to set playback thread priority");
                }
                if is_multi_core_platform && let Some(Some(last_core)) = core_affinity::get_core_ids().map(|ids| ids.last().copied()) {
                    if core_affinity::set_for_current(last_core) {
                        info!("Playback thread set to last core {last_core:?}");
                    } else {
                        warn!("Failed to set playback thread to last core {last_core:?}");
                    }
                }
                let mut retry_count = 0;
                let mut consecutive_failures = 0;
                let result = loop {
                    let Some(song) = queue.get_current_song() else {
                        changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).ok();
                        break PlaybackResult::QueueFinished;
                    };

                    if skip_to_time.load(Ordering::Relaxed) == 0 {
                        metadata_service.increase_play_count(&song.file);
                        if let Some(resume_at) = resume_provider.as_ref().and_then(|p| p.resume_position(&song))
                            && resume_at > 0
                        {
                            info!("Resuming '{}' at {resume_at}s", song.file);
                            skip_to_time.store(resume_at, Ordering::Relaxed);
                        }
                    }

                    if let Err(e) = changes_tx.send(StateChangeEvent::CurrentSongEvent(song.clone())) {
                        warn!("Failed to send current song event: {e}");
                    }
                    if let Err(e) = changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING)) {
                        warn!("Failed to send playback state event: {e}");
                    }
                    let track_loudness = loudness_service.get_loudness(&song.file);
                    let normalization_gain_db: Option<f64> = if rsp_settings.loudness_normalization_enabled {
                        use api_models::settings::NormalizationSource;
                        let tag_track = song.file_tag_track_gain();
                        let tag_album = song.file_tag_album_gain();
                        let calculated = || {
                            track_loudness.map(|lufs_hundredths| {
                                let lufs = f64::from(lufs_hundredths) / 100.0;
                                rsp_settings.loudness_normalization_target_lufs - lufs
                            })
                        };
                        debug!(
                            "Normalization inputs for '{}': source={:?}, track_loudness={:?} (LUFS*100), \
                             tag_track_gain={:?} dB, tag_album_gain={:?} dB, target={} LUFS",
                            song.file,
                            rsp_settings.loudness_normalization_source,
                            track_loudness,
                            tag_track,
                            tag_album,
                            rsp_settings.loudness_normalization_target_lufs,
                        );
                        let gain = match rsp_settings.loudness_normalization_source {
                            NormalizationSource::Calculated => {
                                let g = calculated();
                                debug!("Normalization [Calculated]: gain={g:?} dB");
                                g
                            }
                            NormalizationSource::FileTagsTrack => {
                                debug!("Normalization [FileTagsTrack]: gain={tag_track:?} dB");
                                tag_track
                            }
                            NormalizationSource::FileTagsAlbum => {
                                debug!("Normalization [FileTagsAlbum]: gain={tag_album:?} dB");
                                tag_album
                            }
                            NormalizationSource::Auto => {
                                let g = tag_track.or_else(calculated);
                                if tag_track.is_some() {
                                    debug!("Normalization [Auto]: using file tag track gain={g:?} dB");
                                } else {
                                    debug!("Normalization [Auto]: no track tag, falling back to calculated gain={g:?} dB");
                                }
                                g
                            }
                        };
                        if gain.is_none() {
                            debug!("Normalization: no gain available for '{}', skipping normalization", song.file);
                        }
                        gain
                    } else {
                        None
                    };
                    if let Some(ref dsp) = dsp_handle
                        && let Ok(mut g) = dsp.normalization_gain_db.lock()
                    {
                        *g = normalization_gain_db;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let normalization_gain_hundredths = normalization_gain_db.map(|g| (g * 100.0) as i32);

                    let config = PlaybackConfig::new(audio_device.clone(), rsp_settings.clone(), music_dirs.clone());

                    let mut context = PlaybackContext::new(
                        stop_signal.clone(),
                        skip_to_time.clone(),
                        software_gain.clone(),
                        changes_tx.clone(),
                        dsp_handle.clone(),
                        vu_meter_enabled,
                        sync_tee.clone(),
                    );
                    context.fallback_duration = song.time;

                    let attempt_started = std::time::Instant::now();
                    let play_result = if local_browser_playback {
                        loop {
                            if context.is_stopped() {
                                break Ok(PlaybackResult::PlaybackStopped);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                    } else {
                        super::symphonia::play_file(&song.file, &config, &mut context, track_loudness, normalization_gain_hundredths)
                    };
                    match play_result {
                        Ok(PlaybackResult::PlaybackStopped) => {
                            changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).ok();
                            break PlaybackResult::PlaybackStopped;
                        }
                        Err(err) => {
                            let failed_at = context.position_secs;
                            if attempt_started.elapsed() >= STREAM_RETRY_RESET {
                                retry_count = 0;
                            }
                            let retryable = is_http_stream(&song.file) && !err.is::<OutputUnavailable>();
                            if retryable && retry_count < MAX_STREAM_RETRIES && !stop_signal.load(Ordering::Relaxed) {
                                retry_count += 1;
                                let delay_secs = 1u64 << (retry_count - 1);
                                warn!(
                                    "Stream {} failed at {failed_at}s, reconnecting in {delay_secs}s ({retry_count}/{MAX_STREAM_RETRIES}). Error: {err:?}",
                                    song.file
                                );
                                // One message per outage; the final failure reports itself.
                                if retry_count == 1 {
                                    changes_tx
                                        .send(StateChangeEvent::NotificationError("Stream interrupted, reconnecting...".to_string()))
                                        .ok();
                                }
                                // Sleep in small increments so we can respond to stop_signal quickly.
                                for _ in 0..delay_secs * 10 {
                                    if stop_signal.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                                if stop_signal.load(Ordering::Relaxed) {
                                    changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).ok();
                                    break PlaybackResult::PlaybackStopped;
                                }
                                // Resume where it dropped (podcasts); sources that
                                // cannot seek (live radio) ignore it.
                                if failed_at > 0 {
                                    skip_to_time.store(u16::try_from(failed_at).unwrap_or(u16::MAX), Ordering::Relaxed);
                                }
                                continue;
                            }
                            error!("Failed to play file {}. Error: {:?}", song.file, err);
                            // Stop/Next/Prev arrived meanwhile: the command side
                            // moves the queue, skipping here would advance twice.
                            if stop_signal.load(Ordering::Relaxed) {
                                changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).ok();
                                break PlaybackResult::PlaybackStopped;
                            }
                            consecutive_failures += 1;
                            // Every other song would fail on the same output.
                            let stop = err.is::<OutputUnavailable>() || consecutive_failures >= MAX_CONSECUTIVE_FAILURES;
                            if stop {
                                let reason = if err.is::<OutputUnavailable>() {
                                    format!("{}: {err}", song.file)
                                } else {
                                    format!("{} ({consecutive_failures} songs in a row failed, stopping)", song.file)
                                };
                                changes_tx
                                    .send(StateChangeEvent::PlaybackStateEvent(PlayerState::ERROR(reason)))
                                    .ok();
                                break PlaybackResult::PlaybackFailed;
                            }
                            changes_tx
                                .send(StateChangeEvent::NotificationError(format!(
                                    "Failed to play {}, skipping to the next song",
                                    song.file
                                )))
                                .ok();
                        }
                        res => {
                            info!("Playback finished with result {res:?}");
                            consecutive_failures = 0;
                        }
                    }

                    retry_count = 0;
                    if !queue.move_current_to_next_song() {
                        queue_finished.store(true, Ordering::Relaxed);
                        changes_tx.send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED)).ok();
                        break PlaybackResult::QueueFinished;
                    }
                };
                loudness_service.set_playback_active(false);
                result
            })
            .expect("failed to spawn playback thread")
    }

    fn get_last_played_song_time(&self) -> u16 {
        u16::try_from(Self::read_last_played_song_time(&self.state_db)).unwrap_or_default()
    }

    fn read_last_played_song_time(state_db: &Keyspace) -> u32 {
        match state_db.get(LAST_SONG_PROGRESS_KEY) {
            Ok(Some(lt)) => std::str::from_utf8(&lt).ok().and_then(|s| s.parse().ok()).unwrap_or(0),
            _ => 0,
        }
    }
}
