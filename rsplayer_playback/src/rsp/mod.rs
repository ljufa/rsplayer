use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering}, Mutex,
    },
    time::Duration,
};

use log::{error, info, warn};
use mockall_double::double;
use tokio::task::JoinHandle;

use api_models::{
    num_traits::ToPrimitive,
    settings::Settings,
    state::{PlayerInfo, PlayerState, SongProgress},
};
#[double]
use rsplayer_metadata::metadata::MetadataService;
use rsplayer_metadata::queue::QueueService;

use self::symphonia::PlaybackResult;

mod output;
mod symphonia;
// #[cfg(test)]
// mod test;

pub struct PlayerService {
    queue_service: Arc<QueueService>,
    #[allow(dead_code)]
    metadata_service: Arc<MetadataService>,
    play_handle: Arc<Mutex<Option<JoinHandle<PlaybackResult>>>>,

    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    time: Arc<Mutex<(u64, u64)>>,
    #[allow(clippy::type_complexity)]
    codec_params: Arc<Mutex<(Option<u32>, Option<u32>, Option<usize>, Option<String>)>>,
    audio_device: String,
    buffer_size_mb: usize,
    music_dir: String,
}
impl PlayerService {
    #[must_use]
    pub fn new(
        settings: &Settings,
        metadata_service: Arc<MetadataService>,
        queue_service: Arc<QueueService>,
    ) -> Self {
        PlayerService {
            queue_service,
            metadata_service,
            play_handle: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            time: Arc::new(Mutex::new((0, 0))),
            codec_params: Arc::new(Mutex::new((None, None, None, None))),
            audio_device: settings.alsa_settings.output_device.name.clone(),
            buffer_size_mb: settings.rs_player_settings.buffer_size_mb,
            music_dir: settings.metadata_settings.music_directory.clone(),
        }
    }

    pub fn play_from_current_queue_song(&self) {
        if self.is_paused() {
            let this = self;
            this.paused.store(false, Ordering::SeqCst);
        }
        if self.is_playing() {
            return;
        }
        *self.play_handle.lock().unwrap() = Some(self.play_all_in_queue());
    }

    pub fn pause_current_song(&self) {
        let this = self;
        this.paused.store(true, Ordering::SeqCst);
    }

    pub async fn play_next_song(&self) {
        if self.queue_service.move_current_to_next_song() {
            self.stop_current_song().await;
            self.play_from_current_queue_song();
        }
    }

    pub async fn play_prev_song(&self) {
        if self.queue_service.move_current_to_previous_song() {
            self.stop_current_song().await;
            self.play_from_current_queue_song();
        }
    }

    pub async fn stop_current_song(&self) {
        let this = self;
        this.running.store(false, Ordering::SeqCst);
        self.await_playing_song_to_finish().await;
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    pub fn seek_current_song(&self, _seconds: i8) {
        // todo!()
    }

    pub async fn play_song(&self, song_id: &str) {
        if self.queue_service.move_current_to(song_id) {
            self.stop_current_song().await;
            self.play_from_current_queue_song();
        }
    }

    pub fn get_player_info(&self) -> api_models::state::PlayerInfo {
        let random_next = self.queue_service.get_random_next();
        let is_playing = self.is_playing();
        let is_paused = self.is_paused();
        let params = {
            self.codec_params
                .lock()
                .as_ref()
                .map(|t| (t.0, t.1, t.2, t.3.clone()))
                .unwrap()
        };
        // currrent_song.
        PlayerInfo {
            state: Some(if !is_playing {
                PlayerState::STOPPED
            } else if is_paused {
                PlayerState::PAUSED
            } else {
                PlayerState::PLAYING
            }),
            random: Some(random_next),
            audio_format_rate: params.0,
            audio_format_bit: params.1,
            audio_format_channels: params.2.map(|c| c.to_u32().unwrap_or_default()),
            codec: params.3,
        }
    }

    pub fn get_song_progress(&self) -> api_models::state::SongProgress {
        let mut time = (0, 0);
        if let Ok(l) = self.time.try_lock() {
            time = (l.0, l.1);
        }
        SongProgress {
            total_time: Duration::from_secs(time.0),
            current_time: Duration::from_secs(time.1),
        }
    }

    pub fn toggle_random_play(&self) {
        self.queue_service.toggle_random_next();
    }

    async fn await_playing_song_to_finish(&self) {
        let Some(a) = self.play_handle.lock().unwrap().take() else {
            return;
        };
        a.abort();
        let _ = a.await;
    }

    fn is_playing(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    fn play_all_in_queue(&self) -> JoinHandle<PlaybackResult> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let paused = self.paused.clone();
        let queue = self.queue_service.clone();
        let audio_device = self.audio_device.clone();
        let buffer_size = self.buffer_size_mb;
        let time = self.time.clone();
        let codec_params = self.codec_params.clone();
        let music_dir = self.music_dir.clone();
        let queue_size = queue.get_all_songs().len();
        tokio::task::spawn_blocking(move || {
            let mut num_failed = 0;
            loop {
                let Some(song) = queue.get_current_song() else {
                    running.store(false, Ordering::SeqCst);
                    break PlaybackResult::QueueFinished;
                };
                match symphonia::play_file(
                    &song.file,
                    &running,
                    &paused,
                    &time,
                    &codec_params,
                    &audio_device,
                    buffer_size,
                    &music_dir,
                ) {
                    Ok(PlaybackResult::PlaybackStopped) => {
                        running.store(false, Ordering::SeqCst);
                        break PlaybackResult::PlaybackStopped;
                    }
                    Err(err) => {
                        error!("Failed to play file {}. Error: {:?}", song.file, err);
                        num_failed += 1;
                        if num_failed == 10 || num_failed >= queue_size {
                            warn!("Number of failed songs is greater than 10. Aborting.");
                            running.store(false, Ordering::SeqCst);
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
            }
        })
    }
}
