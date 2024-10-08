use std::sync::{
    atomic::{AtomicBool, AtomicU16, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;

use log::{debug, error, info, warn};
use sled::Db;
use thread_priority::{ThreadBuilder, ThreadPriority};
use tokio::sync::broadcast::Sender;

use api_models::{
    settings::{RsPlayerSettings, Settings},
    state::{PlayerState, StateChangeEvent},
};
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::queue_service::QueueService;

use super::symphonia::PlaybackResult;

pub struct PlayerService {
    state_db: Db,
    queue_service: Arc<QueueService>,
    #[allow(dead_code)]
    metadata_service: Arc<MetadataService>,
    play_handle: Arc<Mutex<Option<JoinHandle<PlaybackResult>>>>,
    running: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    skip_to_time: Arc<AtomicU16>,
    audio_device: String,
    rsp_settings: RsPlayerSettings,
    music_dir: String,
    changes_tx: Sender<StateChangeEvent>,
}
impl PlayerService {
    #[must_use]
    pub fn new(
        settings: &Settings,
        metadata_service: Arc<MetadataService>,
        queue_service: Arc<QueueService>,
        state_changes_tx: Sender<StateChangeEvent>,
    ) -> Self {
        let db = sled::open("player_state").expect("Failed to open queue db");
        let db2 = db.clone();
        let mut rx = state_changes_tx.subscribe();
        tokio::task::spawn(async move {
            let mut i = 0;
            loop {
                if let Ok(StateChangeEvent::SongTimeEvent(st)) = rx.recv().await {
                    i += 1;
                    if i % 5 == 0 {
                        let lt = st.current_time.as_secs().to_string();
                        debug!("Save state: {lt}");
                        _ = db2.insert("last_played_song_progress", lt.as_bytes());
                    }
                }
            }
        });
        let ps = PlayerService {
            state_db: db,
            changes_tx: state_changes_tx,
            queue_service,
            metadata_service,
            play_handle: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            skip_to_time: Arc::new(AtomicU16::new(0)),
            audio_device: settings.alsa_settings.output_device.name.clone(),
            rsp_settings: settings.rs_player_settings.clone(),
            music_dir: settings.metadata_settings.music_directory.clone(),
            stopped: Arc::new(AtomicBool::new(true)),
        };
        let last_played_song_progress = ps.get_last_played_song_time();
        if last_played_song_progress > 0 {
            ps.seek_current_song(last_played_song_progress);
        }
        ps
    }

    pub fn play_from_current_queue_song(&self) {
        if self.is_paused() {
            let this = self;
            this.paused.store(false, Ordering::Relaxed);
        }
        if self.is_playing() {
            self.changes_tx
                .send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING))
                .ok();
            return;
        }
        if let Some(s) = self.queue_service.get_current_song() {
            self.metadata_service.increase_play_count(&s.file);
        }
        *self.play_handle.lock().unwrap() = Some(self.play_all_in_queue());
    }

    pub fn pause_current_song(&self) {
        let this = self;
        this.paused.store(true, Ordering::Relaxed);
    }

    pub fn play_next_song(&self) {
        if self.queue_service.move_current_to_next_song() {
            self.stop_current_song();
            self.play_from_current_queue_song();
        }
    }

    pub fn play_prev_song(&self) {
        if self.queue_service.move_current_to_previous_song() {
            self.stop_current_song();
            self.play_from_current_queue_song();
        }
    }

    pub fn stop_current_song(&self) {
        let this = self;
        this.running.store(false, Ordering::Relaxed);
        self.await_playing_song_to_finish();
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    pub fn seek_current_song(&self, seconds: u16) {
        self.skip_to_time.store(seconds, Ordering::Relaxed);
    }

    pub fn play_song(&self, song_id: &str) {
        if self.queue_service.move_current_to(song_id) {
            self.stop_current_song();
            self.play_from_current_queue_song();
        }
    }

    fn await_playing_song_to_finish(&self) {
        while !self.stopped.load(Ordering::Relaxed) {
            continue;
        }
        debug!("aWait finished");
    }

    fn is_playing(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    fn play_all_in_queue(&self) -> JoinHandle<PlaybackResult> {
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let stopped = self.stopped.clone();
        let paused = self.paused.clone();
        let skip_to_time = self.skip_to_time.clone();
        let queue = self.queue_service.clone();
        let audio_device = self.audio_device.clone();
        let playback_thread_prio = self.rsp_settings.player_threads_priority;
        let music_dir = self.music_dir.clone();
        let queue_size = queue.get_all_songs().len();
        let changes_tx = self.changes_tx.clone();
        let rsp_settings = self.rsp_settings.clone();
        let is_multi_core_platform = core_affinity::get_core_ids().map_or(false, |ids| ids.len() > 1);
        let prio = if is_multi_core_platform {
            ThreadPriority::Crossplatform(playback_thread_prio.try_into().unwrap())
        } else {
            ThreadPriority::Min
        };
        ThreadBuilder::default()
            .name("playback".to_string())
            .priority(prio)
            .spawn(move |prio| {
                if prio.is_ok() {
                    info!("Playback thread started with priority {:?}", playback_thread_prio);
                } else {
                    warn!("Failed to set playback thread priority");
                }
                if is_multi_core_platform {
                    if let Some(Some(last_core)) = core_affinity::get_core_ids().map(|ids| ids.last().copied()) {
                        if core_affinity::set_for_current(last_core) {
                            info!("Playback thread set to last core {:?}", last_core);
                        } else {
                            warn!("Failed to set playback thread to last core {:?}", last_core);
                        }
                    }
                }
                let mut num_failed = 0;
                let result = loop {
                    let Some(song) = queue.get_current_song() else {
                        running.store(false, Ordering::Relaxed);
                        changes_tx
                            .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                            .ok();
                        break PlaybackResult::QueueFinished;
                    };
                    stopped.store(false, Ordering::Relaxed);
                    changes_tx
                        .send(StateChangeEvent::CurrentSongEvent(song.clone()))
                        .expect("msg send failed");
                    changes_tx
                        .send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING))
                        .expect("msg send failed");
                    match super::symphonia::play_file(
                        &song.file,
                        &running,
                        &paused,
                        &skip_to_time,
                        &audio_device,
                        &rsp_settings,
                        &music_dir,
                        &changes_tx,
                    ) {
                        Ok(PlaybackResult::PlaybackStopped) => {
                            running.store(false, Ordering::Relaxed);
                            changes_tx
                                .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                                .ok();
                            break PlaybackResult::PlaybackStopped;
                        }
                        Err(err) => {
                            error!("Failed to play file {}. Error: {:?}", song.file, err);
                            num_failed += 1;
                            if song.file.starts_with("http") || num_failed == 10 || num_failed >= queue_size {
                                warn!("Number of failed songs is greater than 10. Aborting.");
                                running.store(false, Ordering::Relaxed);
                                changes_tx
                                    .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                                    .ok();
                                break PlaybackResult::QueueFinished;
                            }
                        }
                        res => {
                            info!("Playback finished with result {:?}", res);
                            num_failed = 0;
                        }
                    }

                    if !queue.move_current_to_next_song() {
                        break PlaybackResult::QueueFinished;
                    }
                };
                stopped.store(true, Ordering::Relaxed);
                result
            })
            .unwrap()
    }

    fn get_last_played_song_time(&self) -> u16 {
        let last_time = match self.state_db.get("last_played_song_progress") {
            Ok(Some(lt)) => {
                let v = lt.to_vec();
                String::from_utf8(v).unwrap()
            }
            _ => "0".to_string(),
        };
        last_time.parse::<u16>().unwrap_or_default()
    }
}
