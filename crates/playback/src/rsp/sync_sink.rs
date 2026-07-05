//! Follower-side scheduled PCM playback for multiroom.
//!
//! The sync service feeds [`ScheduledChunk`]s (already converted to the
//! local monotonic clock) into a bounded channel; a dedicated thread opens
//! the audio device with the stream's spec and plays each sample at its
//! scheduled time. All of the existing output machinery applies — device
//! rate resampling, channel mapping, the follower's own EQ, VU meter and
//! software volume.
//!
//! Scheduling uses one sensor: for the next sample to be pushed,
//! `error = (now + playback_lag) − scheduled_time`. Large errors are
//! corrected in full (initial alignment, seeks, dropped-chunk gaps); small
//! errors — DAC drift, leader timeline corrections — are slewed gradually
//! by time-stretching chunks a fraction of a percent, inaudibly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use api_models::settings::RsPlayerSettings;
use api_models::state::StateChangeEvent;
use log::{debug, info, warn};
use symphonia::core::audio::{AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef};
use thread_priority::{ThreadBuilder, ThreadPriority};
use tokio::sync::broadcast::Sender;

use crate::rsp::alsa_output::AlsaOutput;
use crate::rsp::tee::MonoClock;
use crate::rsp::vumeter::VUMeter;
use dsp::DspHandle;

/// Frames per write into the output — also the resampler chunk size.
const CHUNK_FRAMES: usize = 4096;
/// Errors beyond this are candidates for a full one-shot correction.
const HARD_CORRECTION_MICROS: i64 = 20_000;
/// Start slewing once the filtered error exceeds this.
const SLEW_START_MICROS: i64 = 5_000;
/// Stop slewing once the filtered error falls back below this. The gap to
/// [`SLEW_START_MICROS`] is hysteresis: the sensor noise is a few ms, and
/// without it corrections toggle on and off every few chunks.
const SLEW_STOP_MICROS: i64 = 1_500;
/// Gradual correction rate: frames adjusted per 1024 frames written (~0.8%).
const SLEW_PER_1024_FRAMES: usize = 8;
/// Frames faded in after a hard trim/lead so the splice doesn't click.
const SPLICE_FADE_FRAMES: usize = 256;
/// A full correction (other than the initial alignment) requires this many
/// consecutive same-direction out-of-threshold chunks — jumpy latency
/// reports from drivers like HDA Intel PCH must not cause skips.
const HARD_PERSISTENCE_CHUNKS: u32 = 8;
/// After a full correction, no further corrections for this long, giving
/// the device buffer and the latency filter time to settle.
const HARD_COOLDOWN_MICROS: u64 = 2_000_000;
/// After the initial alignment the driver's latency reports still ramp
/// (`PipeWire` buffers filling); correcting against them fights a lying
/// sensor and crackles. Hold all corrections until it froze.
const WARMUP_COOLDOWN_MICROS: u64 = 7_000_000;
/// How often the scheduling error is logged at debug level.
const REPORT_INTERVAL_MICROS: u64 = 10_000_000;

/// One network chunk, scheduled on the local monotonic clock.
pub struct ScheduledChunk {
    /// When the first frame should reach the output ([`MonoClock`] µs).
    pub local_play_at_micros: u64,
    /// Interleaved f32 samples at the session spec.
    pub samples: Vec<f32>,
}

pub struct SyncSinkConfig {
    pub rate: u32,
    pub channels: u8,
    /// Loudness-normalization gain from the leader, hundredths of dB.
    pub gain_db_hundredths: Option<i32>,
    /// Per-room trim: positive plays this room later.
    pub latency_offset_ms: i32,
    pub audio_device: String,
    pub rsp_settings: RsPlayerSettings,
}

/// Handle to the running sink thread.
pub struct SyncSink {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SyncSink {
    /// Spawns the sink thread. `rx` is expected to deliver chunks roughly
    /// `buffer_ms` ahead of their scheduled time.
    pub fn start(
        cfg: SyncSinkConfig,
        dsp_handle: Option<DspHandle>,
        software_gain: Option<Arc<AtomicU8>>,
        vu_meter_enabled: bool,
        changes_tx: Sender<StateChangeEvent>,
        rx: Receiver<ScheduledChunk>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        // Configured priority on all platforms — on a single-core RPi Zero
        // the sink must outrank web-UI serving or the ring underruns.
        let prio = cfg.rsp_settings.player_threads_priority;
        let priority = ThreadPriority::Crossplatform(
            prio.try_into()
                .map_err(|e| anyhow::anyhow!("invalid thread priority value: {e}"))?,
        );
        let handle = ThreadBuilder::default()
            .name("multiroom-sink".to_string())
            .priority(priority)
            .spawn(move |prio_result| {
                if prio_result.is_err() {
                    warn!("Failed to set multiroom sink thread priority");
                }
                if let Err(e) = run_sink(&cfg, dsp_handle.as_ref(), software_gain.as_ref(), vu_meter_enabled, &changes_tx, &rx, &stop_thread) {
                    warn!("Multiroom sink stopped with error: {e:#}");
                    let _ = changes_tx.send(StateChangeEvent::NotificationError(format!("Multiroom playback failed: {e}")));
                }
            })
            .context("failed to spawn multiroom sink thread")?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    /// Stops playback immediately and joins the thread.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Waits for the thread to finish on its own — used after dropping the
    /// sender so the sink drains its scheduled tail instead of flushing.
    pub fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SyncSink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_sink(
    cfg: &SyncSinkConfig,
    dsp_handle: Option<&DspHandle>,
    software_gain: Option<&Arc<AtomicU8>>,
    vu_meter_enabled: bool,
    changes_tx: &Sender<StateChangeEvent>,
    rx: &Receiver<ScheduledChunk>,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    // Wait for the first chunk before opening the device.
    let first = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => break chunk,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    };

    let spec = AudioSpec::new(cfg.rate, Channels::Discrete(u16::from(cfg.channels)));
    let (device, is_asio) = crate::rsp::audio_host::find_device(&cfg.audio_device)?;
    let vu_meter = if vu_meter_enabled {
        Some(VUMeter::new(software_gain.cloned(), changes_tx.clone()))
    } else {
        None
    };
    let output = AlsaOutput::new(
        spec.clone(),
        CHUNK_FRAMES as u64,
        &device,
        &cfg.rsp_settings,
        false,
        is_asio,
        dsp_handle,
        vu_meter,
        software_gain,
    )
    .context("failed to open audio output for multiroom sink")?;

    // Give the driver a moment to report its first playback timestamp so
    // the initial alignment already includes the device latency.
    let poll_deadline = std::time::Instant::now() + Duration::from_millis(250);
    while !output.has_latency_measurement() && std::time::Instant::now() < poll_deadline && !stop.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(5));
    }
    if output.has_latency_measurement() && !stop.load(Ordering::Acquire) {
        // Reported latency often ramps up while the server-side buffer fills
        // (PipeWire); let a few more callbacks feed the filter before the
        // initial alignment, or the lead overshoots and gets trimmed later.
        std::thread::sleep(Duration::from_millis(200));
    }
    info!(
        "Multiroom sink opened: {}Hz {}ch on '{}', device latency {}, latency trim {}ms",
        cfg.rate,
        cfg.channels,
        cfg.audio_device,
        if output.has_latency_measurement() {
            format!("{}ms", output.playback_lag_micros() / 1000)
        } else {
            "not reported".to_string()
        },
        cfg.latency_offset_ms
    );

    #[allow(clippy::cast_precision_loss)]
    let mut pipeline = Pipeline {
        output,
        buf: AudioBuffer::<f32>::new(spec, CHUNK_FRAMES),
        channels: usize::from(cfg.channels),
        gain: cfg.gain_db_hundredths.map(|h| 10f32.powf(h as f32 / 100.0 / 20.0)),
        offset_micros: i64::from(cfg.latency_offset_ms) * 1000,
        micros_per_frame: 1_000_000.0 / f64::from(cfg.rate.max(1)),
        hard_corrections: 0,
        last_report_at: 0,
        aligned: false,
        hard_streak: 0,
        hard_streak_sign: 0,
        cooldown_until_micros: 0,
        err_ewma: None,
        slewing: false,
        scratch: Vec::new(),
    };

    pipeline.handle_chunk(&first)?;
    loop {
        if stop.load(Ordering::Acquire) {
            pipeline.output.flush();
            return Ok(());
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => pipeline.handle_chunk(&chunk)?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // Session ended: let the buffered tail play out.
                pipeline.output.drain(stop);
                return Ok(());
            }
        }
    }
}

struct Pipeline {
    output: AlsaOutput,
    buf: AudioBuffer<f32>,
    channels: usize,
    gain: Option<f32>,
    offset_micros: i64,
    micros_per_frame: f64,
    hard_corrections: u64,
    last_report_at: u64,
    /// False until the first chunk aligned the stream (which may correct
    /// immediately); afterwards full corrections need persistent evidence.
    aligned: bool,
    hard_streak: u32,
    hard_streak_sign: i8,
    cooldown_until_micros: u64,
    /// Low-passed scheduling error: the raw per-chunk error sawtooths by a
    /// whole device buffer on drivers without playback timestamps.
    err_ewma: Option<f64>,
    /// True while a gradual correction is in progress (hysteresis state).
    slewing: bool,
    /// Reusable buffer for time-stretched / faded chunks.
    scratch: Vec<f32>,
}

impl Pipeline {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn frames_for_micros(&self, micros: i64) -> usize {
        ((micros.unsigned_abs() as f64) / self.micros_per_frame).round() as usize
    }

    /// Schedules one chunk: measures the error of the next pushed sample
    /// against the chunk's target time and corrects — fully for large,
    /// *persistent* errors, by a bounded slew otherwise — before writing.
    fn handle_chunk(&mut self, chunk: &ScheduledChunk) -> Result<()> {
        let play_at = chunk.local_play_at_micros.saturating_add_signed(self.offset_micros);
        let now = MonoClock::now_micros();
        let will_play_at = now + self.output.playback_lag_micros();
        #[allow(clippy::cast_possible_wrap)]
        let raw_err = will_play_at as i64 - play_at as i64; // >0 → playing late

        // Low-pass the sensor; the first chunk uses the raw value since
        // alignment cannot wait for filter history.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let err = if self.aligned {
            let prev = self.err_ewma.unwrap_or(0.0);
            let filtered = prev + (raw_err as f64 - prev) / 8.0;
            self.err_ewma = Some(filtered);
            filtered.round() as i64
        } else {
            raw_err
        };

        let mut samples: &[f32] = &chunk.samples;
        let frames_in_chunk = samples.len() / self.channels.max(1);
        let was_initial = !self.aligned;
        self.aligned = true;
        if was_initial {
            // Hold all further corrections while the driver's latency
            // reports settle (they ramp on PipeWire/HDA); the frozen sensor
            // gets one clean correction after this window if needed.
            self.cooldown_until_micros = now + WARMUP_COOLDOWN_MICROS;
        }
        let in_cooldown = !was_initial && now < self.cooldown_until_micros;

        let full_correction = if err.abs() <= HARD_CORRECTION_MICROS {
            self.hard_streak = 0;
            false
        } else if was_initial {
            // First chunk: align immediately, whatever it takes.
            true
        } else if in_cooldown {
            // Sensor still settling — hold, don't chase it.
            false
        } else {
            // Noisy drivers (HDA Intel PCH) produce spurious large errors;
            // require sustained same-direction evidence before skipping.
            let sign: i8 = if err > 0 { 1 } else { -1 };
            if sign == self.hard_streak_sign {
                self.hard_streak += 1;
            } else {
                self.hard_streak = 1;
                self.hard_streak_sign = sign;
            }
            self.hard_streak >= HARD_PERSISTENCE_CHUNKS
        };

        // Hysteresis for the gradual corrector: the filtered error still
        // carries a few ms of sensor noise, so a single threshold would
        // toggle the correction on and off chunk by chunk.
        if self.slewing {
            if err.abs() < SLEW_STOP_MICROS {
                self.slewing = false;
            }
        } else if err.abs() > SLEW_START_MICROS {
            self.slewing = true;
        }

        if now.saturating_sub(self.last_report_at) > REPORT_INTERVAL_MICROS {
            self.last_report_at = now;
            debug!(
                "Multiroom sink scheduling error {}µs (hard corrections so far: {})",
                err, self.hard_corrections
            );
        }

        if full_correction {
            self.hard_corrections += 1;
            self.hard_streak = 0;
            self.err_ewma = None; // the correction zeroes the error
            self.slewing = false;
            if !was_initial {
                self.cooldown_until_micros = now + HARD_COOLDOWN_MICROS;
            }
            if err > 0 {
                // Late (late join, stall): skip what already played elsewhere.
                let trim = self.frames_for_micros(err) * self.channels;
                debug!("Multiroom sink late by {}ms, trimming", err / 1000);
                if trim >= samples.len() {
                    return Ok(()); // entire chunk is in the past
                }
                samples = &samples[trim..];
            } else {
                // Early (initial lead, dropped-chunk gap): lead with silence.
                // Pushing blocks once the ring is full, which paces us.
                let lead = self.frames_for_micros(err);
                debug!("Multiroom sink leading with {}ms of silence", (-err) / 1000);
                self.write_silence(lead)?;
            }
            // The splice lands mid-waveform; fade in so it doesn't click.
            let mut out = std::mem::take(&mut self.scratch);
            out.clear();
            out.extend_from_slice(samples);
            fade_in(&mut out, self.channels);
            let res = self.write_samples(&out);
            self.scratch = out;
            return res;
        }

        if !in_cooldown && self.slewing && frames_in_chunk > 1 {
            // Gradual correction: time-stretch the whole chunk by up to
            // ~0.8% instead of splicing frames in or out — a splice is a
            // waveform discontinuity, audible as periodic crackling.
            let adj = self
                .frames_for_micros(err)
                .min((frames_in_chunk * SLEW_PER_1024_FRAMES / 1024).max(1))
                .min(frames_in_chunk / 2);
            if adj > 0 {
                let target = if err > 0 { frames_in_chunk - adj } else { frames_in_chunk + adj };
                let mut out = std::mem::take(&mut self.scratch);
                resample_into(samples, self.channels, target, &mut out);
                // Fold the applied shift into the filtered error right away —
                // the EWMA lags ~8 chunks behind and would overshoot otherwise.
                #[allow(clippy::cast_precision_loss)]
                let applied = adj as f64 * self.micros_per_frame * if err > 0 { 1.0 } else { -1.0 };
                if let Some(e) = self.err_ewma.as_mut() {
                    *e -= applied;
                }
                let res = self.write_samples(&out);
                self.scratch = out;
                return res;
            }
        }

        self.write_samples(samples)
    }

    fn write_samples(&mut self, samples: &[f32]) -> Result<()> {
        for chunk in samples.chunks(CHUNK_FRAMES * self.channels) {
            let frames = chunk.len() / self.channels;
            if frames == 0 {
                continue;
            }
            self.buf.resize_uninit(frames);
            self.buf.copy_from_slice_interleaved(&chunk);
            if let Some(g) = self.gain {
                for plane in self.buf.iter_planes_mut() {
                    for s in plane {
                        *s *= g;
                    }
                }
            }
            self.output.write(GenericAudioBufferRef::F32(&self.buf))?;
        }
        Ok(())
    }

    fn write_silence(&mut self, mut frames: usize) -> Result<()> {
        while frames > 0 {
            let n = frames.min(CHUNK_FRAMES);
            self.buf.resize_with_silence(n);
            self.output.write(GenericAudioBufferRef::F32(&self.buf))?;
            frames -= n;
        }
        Ok(())
    }
}

/// Linear fade-in over the first [`SPLICE_FADE_FRAMES`] frames, softening
/// the discontinuity a hard correction splices into the waveform.
fn fade_in(samples: &mut [f32], channels: usize) {
    let channels = channels.max(1);
    let frames = (samples.len() / channels).min(SPLICE_FADE_FRAMES);
    for f in 0..frames {
        #[allow(clippy::cast_precision_loss)]
        let g = f as f32 / frames as f32;
        for s in &mut samples[f * channels..(f + 1) * channels] {
            *s *= g;
        }
    }
}

/// Linear time-stretch of an interleaved chunk to `out_frames`. The first
/// and last input frames map to the first and last output frames, so
/// consecutive chunks stay continuous across the stretch.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn resample_into(input: &[f32], channels: usize, out_frames: usize, out: &mut Vec<f32>) {
    let channels = channels.max(1);
    let in_frames = input.len() / channels;
    out.clear();
    if in_frames < 2 || out_frames < 2 {
        out.extend_from_slice(input);
        return;
    }
    out.reserve(out_frames * channels);
    let step = (in_frames - 1) as f64 / (out_frames - 1) as f64;
    for j in 0..out_frames {
        let pos = j as f64 * step;
        let i0 = (pos as usize).min(in_frames - 2);
        let frac = (pos - i0 as f64) as f32;
        for c in 0..channels {
            let a = input[i0 * channels + c];
            let b = input[(i0 + 1) * channels + c];
            out.push((b - a).mul_add(frac, a));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resample_into;

    #[test]
    fn resample_preserves_endpoints_and_length() {
        // Stereo ramp: L = frame index, R = negative frame index.
        let channels = 2;
        let input: Vec<f32> = (0..1024).flat_map(|f| [f as f32, -(f as f32)]).collect();
        let mut out = Vec::new();
        for target in [1016usize, 1024, 1032] {
            resample_into(&input, channels, target, &mut out);
            assert_eq!(out.len(), target * channels);
            assert_eq!(out[0], 0.0);
            assert_eq!(out[out.len() - 2], 1023.0);
            assert_eq!(out[out.len() - 1], -1023.0);
            // A ramp must stay a ramp: strictly increasing left channel.
            for w in out.chunks_exact(channels).collect::<Vec<_>>().windows(2) {
                assert!(w[1][0] > w[0][0]);
            }
        }
    }

    #[test]
    fn resample_degenerate_inputs_pass_through() {
        let mut out = vec![7.0f32];
        resample_into(&[1.0, 2.0], 2, 5, &mut out); // one frame only
        assert_eq!(out, vec![1.0, 2.0]);
        resample_into(&[1.0, 2.0, 3.0, 4.0], 2, 1, &mut out); // target too small
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }
}

