use std::{
    fs::File,
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
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF, CODEC_TYPE_NULL},
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::{Hint, ProbeResult},
};

use crate::loudness_repository::LoudnessRepository;
use crate::song_repository::SongRepository;

pub struct LoudnessService {
    repository: Arc<LoudnessRepository>,
    song_repository: Arc<SongRepository>,
    music_dirs: Vec<String>,
    /// Set to `true` by `PlayerService` while the playback thread is running.
    /// The scan loop suspends itself whenever this is `true`.
    pub is_playing: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl LoudnessService {
    pub fn new(repository: Arc<LoudnessRepository>, song_repository: Arc<SongRepository>, music_dirs: Vec<String>) -> Arc<Self> {
        Arc::new(Self {
            repository,
            song_repository,
            music_dirs,
            is_playing: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Spawn the background scan thread and its rayon worker pool.
    /// Safe to call once after construction. No threads are created until this is called.
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

    /// Look up the stored loudness in hundredths of a LUFS.
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
                thread::sleep(Duration::from_secs(60));
                continue;
            }

            // Wait for any active playback to finish before starting the batch.
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

            thread_pool.install(|| pending.par_iter().for_each(|file_key| {
                if self.stop.load(Ordering::Relaxed) {
                    return;
                }
                // If playback starts mid-batch, skip this file — it will be
                // picked up in the next outer-loop iteration.
                if self.is_playing.load(Ordering::Relaxed) {
                    debug!("Loudness scan: skipping {file_key} (playback started)");
                    return;
                }

                let full_path = self.music_dirs.iter()
                    .map(|dir| format!("{dir}/{file_key}"))
                    .find(|p| Path::new(p).exists());
                let Some(full_path) = full_path else {
                    warn!("Loudness scan: file not found in any music directory: {file_key}");
                    return;
                };
                debug!("Loudness scan: analysing {file_key}");
                if let Some(lufs) = measure_integrated_loudness(Path::new(&full_path)) {
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
            }));

            self.repository.flush();
            let total = scanned.load(Ordering::Relaxed);
            info!("Loudness scan: pass complete, {total} songs analysed");
        }
    }
}

fn measure_integrated_loudness(file_path: &Path) -> Option<f64> {
    let file = Box::new(File::open(file_path).ok()?);
    let mss = MediaSourceStream::new(file, MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = file_path.extension() {
        hint.with_extension(ext.to_str().unwrap_or(""));
    }

    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;

    measure_from_probed(&mut probed)
}

fn measure_from_probed(probed: &mut ProbeResult) -> Option<f64> {
    let track = probed.format.default_track()?;
    let track_id = track.id;
    let codec = track.codec_params.codec;

    if codec == CODEC_TYPE_NULL || codec == CODEC_TYPE_DSD_LSBF || codec == CODEC_TYPE_DSD_MSBF {
        return None;
    }

    let channels = u32::try_from(track.codec_params.channels?.count()).ok()?;
    let sample_rate = track.codec_params.sample_rate?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut meter = ebur128::EbuR128::new(channels, sample_rate, ebur128::Mode::I).ok()?;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let Ok(audio_buf) = decoder.decode(&packet) else { continue };

        let spec = *audio_buf.spec();
        let frames = audio_buf.frames();
        let cap = audio_buf.capacity() as u64;
        if sample_buf.as_ref().is_none_or(|sb| sb.capacity() < frames) {
            sample_buf = Some(SampleBuffer::<f32>::new(cap, spec));
        }
        let sbuf = sample_buf.as_mut().unwrap();
        sbuf.copy_interleaved_ref(audio_buf);
        let _ = meter.add_frames_f32(sbuf.samples());
    }

    meter.loudness_global().ok()
}
