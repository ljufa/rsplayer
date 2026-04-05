use fjall::{Database, Keyspace, KeyspaceCreateOptions};
use log::{debug, error, info, trace, warn};
use std::sync::{
    atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use thread_priority::{ThreadBuilder, ThreadPriority};
use tokio::sync::broadcast::Sender;

use api_models::{
    settings::{DspSettings, RsPlayerSettings, Settings},
    state::{PlayerState, StateChangeEvent},
};
use rsplayer_metadata::loudness_service::LoudnessService;
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::queue_service::QueueService;

use rsplayer_dsp::DspProcessor;

use super::symphonia::PlaybackResult;
use crate::rsp::playback_config::PlaybackConfig;
use crate::rsp::playback_context::PlaybackContext;

pub struct PlayerService {
    state_db: Keyspace,
    queue_service: Arc<QueueService>,
    metadata_service: Arc<MetadataService>,
    playback_thread_handle: Arc<Mutex<Option<JoinHandle<PlaybackResult>>>>,
    stop_signal: Arc<AtomicBool>,
    skip_to_time: Arc<AtomicU16>,
    last_known_time: Arc<AtomicU32>,
    current_volume: Arc<AtomicU8>,
    audio_device: String,
    rsp_settings: RsPlayerSettings,
    local_browser_playback: bool,
    changes_tx: Sender<StateChangeEvent>,
    dsp_processor: Arc<Mutex<Option<DspProcessor>>>,
    loudness_service: Arc<LoudnessService>,
}

const LAST_SONG_PAUSED_KEY: &str = "last_song_paused";
const LAST_SONG_PROGRESS_KEY: &str = "last_played_song_progress";

impl PlayerService {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new(
        db: &Database,
        settings: &Settings,
        metadata_service: Arc<MetadataService>,
        queue_service: Arc<QueueService>,
        state_changes_tx: Sender<StateChangeEvent>,
        loudness_service: Arc<LoudnessService>,
    ) -> Self {
        let state_db = db
            .keyspace("player_state", KeyspaceCreateOptions::default)
            .expect("Failed to open player_state keyspace");
        let state_db_async = state_db.clone();
        let mut rx = state_changes_tx.subscribe();
        let state_tx = state_changes_tx.clone();

        let initial_time = match state_db.get(LAST_SONG_PROGRESS_KEY) {
            Ok(Some(lt)) => String::from_utf8(lt.to_vec())
                .unwrap_or_else(|_| "0".to_string())
                .parse::<u32>()
                .unwrap_or(0),
            _ => 0,
        };
        let last_known_time = Arc::new(AtomicU32::new(initial_time));
        let last_known_time_clone = last_known_time.clone();
        let current_volume = Arc::new(AtomicU8::new(0));
        let current_volume_clone = current_volume.clone();
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

        tokio::task::spawn(async move {
            let mut last_saved_secs: u64 = u64::MAX;
            loop {
                match rx.recv().await {
                    Ok(StateChangeEvent::SongTimeEvent(st)) => {
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
                    Ok(StateChangeEvent::PlaybackStateEvent(ps)) => {
                        debug!("Save player state: {ps:?}");
                        match ps {
                            PlayerState::PLAYING => {
                                _ = state_db_async.remove(LAST_SONG_PAUSED_KEY);
                                state_tx
                                    .send(StateChangeEvent::NotificationSuccess("Playing".to_string()))
                                    .ok();
                            }
                            PlayerState::PAUSED | PlayerState::STOPPED => {
                                _ = state_db_async.insert(LAST_SONG_PAUSED_KEY, "true");
                                state_tx
                                    .send(StateChangeEvent::NotificationSuccess("Playback paused".to_string()))
                                    .ok();
                            }
                            PlayerState::ERROR(msg) => {
                                state_tx
                                    .send(StateChangeEvent::NotificationError(format!("Failed to play {msg}")))
                                    .ok();
                            }
                        }
                    }
                    Ok(StateChangeEvent::VolumeChangeEvent(vol)) => {
                        current_volume_clone.store(vol.current, Ordering::Relaxed);
                    }
                    Ok(StateChangeEvent::PlayerInfoEvent(info)) => {
                        if let Ok(mut guard) = dsp_processor_clone.lock() {
                            if let Some(proc) = guard.as_mut() {
                                if let Some(rate) = info.audio_format_rate {
                                    proc.rate = rate as usize;
                                }
                                if let Some(channels) = info.audio_format_channels {
                                    proc.channels = channels;
                                }
                                info!(
                                    "DSP processor updated with rate: {}, channels: {}",
                                    proc.rate, proc.channels
                                );
                            }
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
            skip_to_time: Arc::new(AtomicU16::new(0)),
            last_known_time,
            current_volume,
            audio_device: settings.alsa_settings.output_device.name.clone(),
            rsp_settings: settings.rs_player_settings.clone(),
            local_browser_playback: settings.local_browser_playback,
            dsp_processor,
            loudness_service,
        };
        let last_played_song_progress = ps.get_last_played_song_time();
        if last_played_song_progress > 0 {
            ps.seek_current_song(last_played_song_progress);
        }
        ps
    }

    pub fn play_from_current_queue_song(&self) {
        if let Ok(Some(_)) = self.state_db.get(LAST_SONG_PAUSED_KEY) {
            let last_song_time = self.get_last_played_song_time();
            self.seek_current_song(last_song_time);
        }

        *self.playback_thread_handle.lock().unwrap() = Some(self.play_all_in_queue());
    }

    pub fn play_from_beginning(&self) {
        _ = self.state_db.remove(LAST_SONG_PAUSED_KEY);
        *self.playback_thread_handle.lock().unwrap() = Some(self.play_all_in_queue());
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
        let handle = self.playback_thread_handle.lock().unwrap().take();
        handle.and_then(|h| h.join().ok())
    }

    pub fn toggle_play_pause(&self) {
        if self.stop_signal.load(Ordering::Relaxed) {
            self.play_from_current_queue_song();
        } else {
            self.stop_current_song();
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
            if let Some(ref mut dsp) = *guard {
                dsp.update_settings(dsp_settings);
            } else {
                debug!("DSP update requested but DSP is disabled");
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn play_all_in_queue(&self) -> JoinHandle<PlaybackResult> {
        self.stop_signal.store(false, Ordering::Relaxed);
        let stop_signal = self.stop_signal.clone();
        let skip_to_time = self.skip_to_time.clone();
        let queue = self.queue_service.clone();
        let audio_device = self.audio_device.clone();
        let playback_thread_prio = self.rsp_settings.player_threads_priority;
        let music_dirs = self.metadata_service.effective_directories();
        let changes_tx = self.changes_tx.clone();
        let rsp_settings = self.rsp_settings.clone();
        let metadata_service = self.metadata_service.clone();
        let dsp_handle = self
            .dsp_processor
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(DspProcessor::handle));
        let current_volume = self.current_volume.clone();
        let vu_meter_enabled = self.rsp_settings.vu_meter_enabled;
        let loudness_service = self.loudness_service.clone();
        let is_multi_core_platform = core_affinity::get_core_ids().is_some_and(|ids| ids.len() > 1);
        let local_browser_playback = self.local_browser_playback;
        let prio = if is_multi_core_platform {
            ThreadPriority::Crossplatform(playback_thread_prio.try_into().unwrap())
        } else {
            ThreadPriority::Min
        };
        ThreadBuilder::default()
            .name("playback".to_string())
            .priority(prio)
            .spawn(move |prio| {
                const MAX_RETRIES: i32 = 5;
                loudness_service.set_playback_active(true);
                if prio.is_ok() {
                    info!("Playback thread started with priority {playback_thread_prio:?}");
                } else {
                    warn!("Failed to set playback thread priority");
                }
                if is_multi_core_platform {
                    if let Some(Some(last_core)) = core_affinity::get_core_ids().map(|ids| ids.last().copied()) {
                        if core_affinity::set_for_current(last_core) {
                            info!("Playback thread set to last core {last_core:?}");
                        } else {
                            warn!("Failed to set playback thread to last core {last_core:?}");
                        }
                    }
                }
                let mut retry_count = 0;
                let result = loop {
                    let Some(song) = queue.get_current_song() else {
                        changes_tx
                            .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                            .ok();
                        break PlaybackResult::QueueFinished;
                    };

                    if skip_to_time.load(Ordering::Relaxed) == 0 {
                        metadata_service.increase_play_count(&song.file);
                    }

                    changes_tx
                        .send(StateChangeEvent::CurrentSongEvent(song.clone()))
                        .expect("msg send failed");
                    changes_tx
                        .send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING))
                        .expect("msg send failed");
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
                                    debug!(
                                        "Normalization [Auto]: no track tag, falling back to calculated gain={g:?} dB"
                                    );
                                }
                                g
                            }
                        };
                        if gain.is_none() {
                            debug!(
                                "Normalization: no gain available for '{}', skipping normalization",
                                song.file
                            );
                        }
                        gain
                    } else {
                        None
                    };
                    if let Some(ref dsp) = dsp_handle {
                        if let Ok(mut g) = dsp.normalization_gain_db.lock() {
                            *g = normalization_gain_db;
                        }
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let normalization_gain_hundredths = normalization_gain_db.map(|g| (g * 100.0) as i32);

                    let config = PlaybackConfig::new(
                        audio_device.clone(),
                        rsp_settings.clone(),
                        music_dirs.clone(),
                        vu_meter_enabled,
                        local_browser_playback,
                    );

                    let mut context = PlaybackContext::new(
                        stop_signal.clone(),
                        skip_to_time.clone(),
                        current_volume.clone(),
                        changes_tx.clone(),
                        dsp_handle.clone(),
                        vu_meter_enabled,
                    );

                    let play_result = if local_browser_playback {
                        loop {
                            if context.is_stopped() {
                                break Ok(PlaybackResult::PlaybackStopped);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                    } else {
                        super::symphonia::play_file(
                            &song.file,
                            &config,
                            &mut context,
                            track_loudness,
                            normalization_gain_hundredths,
                        )
                    };
                    match play_result {
                        Ok(PlaybackResult::PlaybackStopped) => {
                            changes_tx
                                .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                                .ok();
                            break PlaybackResult::PlaybackStopped;
                        }
                        Err(err) => {
                            if retry_count < MAX_RETRIES && !stop_signal.load(Ordering::Relaxed) {
                                retry_count += 1;
                                warn!(
                                    "Playback failed, retrying ({retry_count}/{MAX_RETRIES}) in 1s... Error: {err:?}"
                                );
                                changes_tx
                                    .send(StateChangeEvent::NotificationError(format!(
                                        "Retrying ({retry_count}/{MAX_RETRIES})..."
                                    )))
                                    .ok();
                                // Sleep in small increments so we can respond to stop_signal quickly.
                                for _ in 0..10 {
                                    if stop_signal.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                                if stop_signal.load(Ordering::Relaxed) {
                                    changes_tx
                                        .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                                        .ok();
                                    break PlaybackResult::PlaybackStopped;
                                }
                                continue;
                            }
                            error!("Failed to play file {}. Error: {:?}", song.file, err);
                            changes_tx
                                .send(StateChangeEvent::PlaybackStateEvent(PlayerState::ERROR(song.file)))
                                .ok();
                            break PlaybackResult::PlaybackFailed;
                        }
                        res => {
                            info!("Playback finished with result {res:?}");
                        }
                    }

                    retry_count = 0;
                    if !queue.move_current_to_next_song() {
                        break PlaybackResult::QueueFinished;
                    }
                };
                loudness_service.set_playback_active(false);
                result
            })
            .unwrap()
    }

    fn get_last_played_song_time(&self) -> u16 {
        let last_time = match self.state_db.get(LAST_SONG_PROGRESS_KEY) {
            Ok(Some(lt)) => {
                let v = lt.to_vec();
                String::from_utf8(v).unwrap()
            }
            _ => "0".to_string(),
        };
        last_time.parse::<u16>().unwrap_or_default()
    }
}
