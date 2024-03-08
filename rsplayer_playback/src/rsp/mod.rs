use std::sync::{
    atomic::{AtomicBool, AtomicU16, Ordering},
    Arc, Mutex,
};

use log::{error, info, warn};

use tokio::{sync::broadcast::Sender, task::JoinHandle};

use api_models::{
    settings::Settings,
    state::{PlayerState, StateChangeEvent},
};
use rsplayer_metadata::metadata_service::MetadataService;
use rsplayer_metadata::queue_service::QueueService;

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
    skip_to_time: Arc<AtomicU16>,
    audio_device: String,
    buffer_size_mb: usize,
    music_dir: String,
}
impl PlayerService {
    #[must_use]
    pub fn new(settings: &Settings, metadata_service: Arc<MetadataService>, queue_service: Arc<QueueService>) -> Self {
        PlayerService {
            queue_service,
            metadata_service,
            play_handle: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            skip_to_time: Arc::new(AtomicU16::new(0)),
            audio_device: settings.alsa_settings.output_device.name.clone(),
            buffer_size_mb: settings.rs_player_settings.buffer_size_mb,
            music_dir: settings.metadata_settings.music_directory.clone(),
        }
    }

    pub fn play_from_current_queue_song(&self, changes_tx: &Sender<StateChangeEvent>) {
        if self.is_paused() {
            let this = self;
            this.paused.store(false, Ordering::SeqCst);
        }
        if self.is_playing() {
            changes_tx
                .send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING))
                .ok();
            return;
        }
        if let Some(s) = self.queue_service.get_current_song() {
            self.metadata_service.increase_play_count(&s.file);
        }
        *self.play_handle.lock().unwrap() = Some(self.play_all_in_queue(changes_tx));
    }

    pub fn pause_current_song(&self) {
        let this = self;
        this.paused.store(true, Ordering::SeqCst);
    }

    pub async fn play_next_song(&self, changes_tx: &Sender<StateChangeEvent>) {
        if self.queue_service.move_current_to_next_song() {
            self.stop_current_song().await;
            self.play_from_current_queue_song(changes_tx);
        }
    }

    pub async fn play_prev_song(&self, changes_tx: &Sender<StateChangeEvent>) {
        if self.queue_service.move_current_to_previous_song() {
            self.stop_current_song().await;
            self.play_from_current_queue_song(changes_tx);
        }
    }

    pub async fn stop_current_song(&self) {
        let this = self;
        this.running.store(false, Ordering::SeqCst);
        self.await_playing_song_to_finish().await;
    }

    #[allow(clippy::unused_self, clippy::missing_const_for_fn)]
    pub fn seek_current_song(&self, seconds: u16) {
        self.skip_to_time.store(seconds, Ordering::SeqCst);
    }

    pub async fn play_song(&self, song_id: &str, changes_tx: &Sender<StateChangeEvent>) {
        if self.queue_service.move_current_to(song_id) {
            self.stop_current_song().await;
            self.play_from_current_queue_song(changes_tx);
        }
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

    fn play_all_in_queue(&self, changes_tx: &Sender<StateChangeEvent>) -> JoinHandle<PlaybackResult> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let paused = self.paused.clone();
        let skip_to_time = self.skip_to_time.clone();
        let queue = self.queue_service.clone();
        let audio_device = self.audio_device.clone();
        let buffer_size = self.buffer_size_mb;
        let music_dir = self.music_dir.clone();
        let queue_size = queue.get_all_songs().len();
        let changes_tx = changes_tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut num_failed = 0;
            loop {
                let Some(song) = queue.get_current_song() else {
                    running.store(false, Ordering::SeqCst);
                    changes_tx
                        .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                        .ok();
                    break PlaybackResult::QueueFinished;
                };
                changes_tx
                    .send(StateChangeEvent::CurrentSongEvent(song.clone()))
                    .expect("msg send failed");
                changes_tx
                    .send(StateChangeEvent::PlaybackStateEvent(PlayerState::PLAYING))
                    .expect("msg send failed");
                match symphonia::play_file(
                    &song.file,
                    &running,
                    &paused,
                    &skip_to_time,
                    &audio_device,
                    buffer_size,
                    &music_dir,
                    &changes_tx,
                ) {
                    Ok(PlaybackResult::PlaybackStopped) => {
                        running.store(false, Ordering::SeqCst);
                        changes_tx
                            .send(StateChangeEvent::PlaybackStateEvent(PlayerState::STOPPED))
                            .ok();
                        break PlaybackResult::PlaybackStopped;
                    }
                    Err(err) => {
                        error!("Failed to play file {}. Error: {:?}", song.file, err);
                        num_failed += 1;
                        if num_failed == 10 || num_failed >= queue_size {
                            warn!("Number of failed songs is greater than 10. Aborting.");
                            running.store(false, Ordering::SeqCst);
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
            }
        })
    }
}
