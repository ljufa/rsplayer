use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use log::{debug, info, warn};
use rayon::prelude::*;
use std::sync::atomic::AtomicU32;
use std::thread::available_parallelism;

use crate::loudness_analyzer::LoudnessAnalyzer;
use crate::loudness_repository::LoudnessRepository;
use crate::song_repository::SongRepository;

pub struct LoudnessService {
    repository: Arc<LoudnessRepository>,
    song_repository: Arc<SongRepository>,
    music_dirs: Vec<String>,
    pub is_playing: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl LoudnessService {
    pub fn new(
        repository: Arc<LoudnessRepository>,
        song_repository: Arc<SongRepository>,
        music_dirs: Vec<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            repository,
            song_repository,
            music_dirs,
            is_playing: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let svc = self.clone();
        thread::Builder::new()
            .name("loudness-scan".into())
            .spawn(move || {
                let cores = available_parallelism().map_or(1, std::num::NonZero::get);
                let threads = (cores / 2).max(1);
                info!("Loudness scan: using {threads} threads ({cores} cores available)");
                let thread_pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|i| format!("loudness-worker-{i}"))
                    .build()
                    .expect("Failed to build loudness thread pool");
                svc.scan_loop(&thread_pool);
            })
            .expect("Failed to spawn loudness-scan thread");
    }

    pub fn set_playback_active(&self, active: bool) {
        info!("Loudness scan: playback active = {active}");
        self.is_playing.store(active, Ordering::Relaxed);
    }

    pub fn get_loudness(&self, file_key: &str) -> Option<i32> {
        self.repository.get(file_key)
    }

    fn scan_loop(&self, thread_pool: &rayon::ThreadPool) {
        info!("Loudness scan thread started");
        loop {
            if self.stop.load(Ordering::Relaxed) {
                info!("Loudness scan thread stopping");
                break;
            }

            let pending: Vec<String> = self
                .song_repository
                .get_all_iterator()
                .filter(|s| !self.repository.contains(&s.file))
                .map(|s| s.file)
                .collect();

            if pending.is_empty() {
                debug!("Loudness scan: all songs analysed, sleeping 60s");
                thread::sleep(Duration::from_mins(1));
                continue;
            }

            if self.is_playing.load(Ordering::Relaxed) {
                info!("Loudness scan: waiting for playback to stop before next batch");
                while self.is_playing.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(500));
                    if self.stop.load(Ordering::Relaxed) {
                        return;
                    }
                }
                info!("Loudness scan: playback stopped, starting batch");
            }

            info!("Loudness scan: {} songs pending, analysing in parallel", pending.len());
            let scanned = AtomicU32::new(0);

            thread_pool.install(|| {
                pending.par_iter().for_each(|file_key| {
                    if self.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if self.is_playing.load(Ordering::Relaxed) {
                        debug!("Loudness scan: skipping {file_key} (playback started)");
                        return;
                    }

                    let full_path = self
                        .music_dirs
                        .iter()
                        .map(|dir| format!("{dir}/{file_key}"))
                        .find(|p| Path::new(p).exists());
                    let Some(full_path) = full_path else {
                        warn!("Loudness scan: file not found in any music directory: {file_key}");
                        return;
                    };
                    debug!("Loudness scan: analysing {file_key}");
                    if let Some(lufs) = LoudnessAnalyzer::measure_file(Path::new(&full_path)) {
                        #[allow(clippy::cast_possible_truncation)]
                        let stored = (lufs * 100.0).round() as i32;
                        debug!("Loudness scan: {file_key} => {lufs:.2} LUFS");
                        self.repository.save_loudness(file_key, stored);
                    } else {
                        warn!("Loudness scan: no loudness for {file_key} (DSD or unsupported)");
                        self.repository.save_unavailable(file_key);
                    }

                    let c = scanned.fetch_add(1, Ordering::Relaxed) + 1;
                    if c.is_multiple_of(50) {
                        info!("Loudness scan: {c} songs done so far");
                        self.repository.flush();
                    }
                });
            });

            self.repository.flush();
            let total = scanned.load(Ordering::Relaxed);
            info!("Loudness scan: pass complete, {total} songs analysed");
            if total == 0 {
                warn!("Loudness scan: no files could be read (files unavailable?), sleeping 60s before retry");
                thread::sleep(Duration::from_mins(1));
            }
        }
    }
}
