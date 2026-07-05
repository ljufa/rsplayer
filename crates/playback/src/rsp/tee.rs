//! Leader-side PCM tee for multiroom playback.
//!
//! The decode loop in [`super::symphonia::play_file`] copies each decoded
//! packet as interleaved f32 (source rate, pre-EQ/pre-resample/pre-volume)
//! into a bounded channel consumed by the sync service, which timestamps and
//! fans the chunks out to grouped followers.
//!
//! The tee must never degrade local playback: sends are `try_send` and a
//! full channel only costs the followers a gap. Every chunk carries the
//! frame index of its first sample counted on the playback thread, so a
//! dropped chunk shifts nothing — the timeline stays honest.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use log::warn;
use symphonia::core::audio::GenericAudioBufferRef;
use tokio::sync::mpsc;

/// Process-wide monotonic clock in microseconds.
///
/// The playback thread stamps session epochs with it and the sync crate uses
/// the same clock for offset measurement — there must be exactly one epoch
/// per process, which is why it lives here rather than in the sync crate.
pub struct MonoClock;

static EPOCH: OnceLock<Instant> = OnceLock::new();

impl MonoClock {
    #[must_use]
    pub fn now_micros() -> u64 {
        let epoch = *EPOCH.get_or_init(Instant::now);
        u64::try_from(epoch.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// PCM parameters of one tee session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeeSpec {
    pub rate: u32,
    pub channels: u8,
}

/// Events flowing from the playback thread to the sync service.
pub enum TeeEvent {
    SessionStart {
        session_id: u64,
        spec: TeeSpec,
        /// Leader monotonic time at which frame 0 of this session reaches
        /// the leader's own output (the ring buffer was prefilled with this
        /// much silence).
        epoch_micros: u64,
        /// Loudness-normalization gain in hundredths of dB — applied by
        /// followers because the tee happens before the leader's DSP chain.
        gain_db_hundredths: Option<i32>,
    },
    Chunk {
        session_id: u64,
        /// Frame index (per channel) of the first sample, since session start.
        first_frame: u64,
        /// Interleaved f32 samples at the session spec.
        samples: Arc<Vec<f32>>,
    },
    SessionEnd {
        session_id: u64,
    },
    /// Measured difference between where the leader's DAC actually is and
    /// the nominal session timeline (`actual − nominal`, µs). Followers add
    /// it to chunk timestamps, keeping them locked to the leader's real
    /// output as its clock drifts.
    TimelineCorrection {
        session_id: u64,
        offset_micros: i64,
    },
}

/// Handle given to `PlayerService`/`PlaybackContext`; cheap to clone.
#[derive(Clone)]
pub struct SyncTee {
    /// Set by the sync service while at least one follower is grouped.
    active: Arc<AtomicBool>,
    buffer_ms: u32,
    tx: mpsc::Sender<TeeEvent>,
    next_session_id: Arc<AtomicU64>,
}

impl SyncTee {
    /// Creates the tee and the receiving end for the sync service.
    /// Capacity ~256 events holds a couple of seconds of typical packets.
    #[must_use]
    pub fn new(buffer_ms: u32) -> (Self, mpsc::Receiver<TeeEvent>) {
        let (tx, rx) = mpsc::channel(256);
        (
            Self {
                active: Arc::new(AtomicBool::new(false)),
                buffer_ms,
                tx,
                next_session_id: Arc::new(AtomicU64::new(1)),
            },
            rx,
        )
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Shared activity flag — the sync service is the writer.
    #[must_use]
    pub fn active_flag(&self) -> Arc<AtomicBool> {
        self.active.clone()
    }

    #[must_use]
    pub const fn buffer_ms(&self) -> u32 {
        self.buffer_ms
    }

    /// Starts a session whose frame 0 plays locally at `epoch_micros`.
    #[must_use]
    pub fn begin_session(&self, spec: TeeSpec, epoch_micros: u64, gain_db_hundredths: Option<i32>) -> TeeSession {
        let session_id = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.try_send(TeeEvent::SessionStart {
            session_id,
            spec,
            epoch_micros,
            gain_db_hundredths,
        });
        TeeSession {
            session_id,
            rate: spec.rate,
            epoch_micros,
            channels: usize::from(spec.channels),
            frames_sent: 0,
            dropped_chunks: 0,
            last_correction_at: MonoClock::now_micros(),
            offset_sum: 0,
            offset_samples: 0,
            tx: self.tx.clone(),
            interleave_buf: Vec::new(),
        }
    }
}

/// One track's worth of tee output. Ends the session on drop, so every exit
/// path of the decode loop (stop, error, natural end) notifies followers.
pub struct TeeSession {
    session_id: u64,
    rate: u32,
    epoch_micros: u64,
    channels: usize,
    frames_sent: u64,
    dropped_chunks: u64,
    last_correction_at: u64,
    offset_sum: i64,
    offset_samples: u32,
    tx: mpsc::Sender<TeeEvent>,
    interleave_buf: Vec<f32>,
}

impl TeeSession {
    /// Copies a decoded buffer into the tee. Never blocks the playback
    /// thread; on a full channel the chunk is dropped (followers hear a gap)
    /// but the frame counter still advances, keeping timestamps correct.
    pub fn send_chunk(&mut self, decoded: &GenericAudioBufferRef<'_>) {
        let frames = decoded.frames();
        if frames == 0 {
            return;
        }
        let first_frame = self.frames_sent;
        self.frames_sent += frames as u64;

        self.interleave_buf.clear();
        self.interleave_buf.resize(frames * self.channels, 0.0);
        decoded.copy_to_slice_interleaved(&mut self.interleave_buf);

        let event = TeeEvent::Chunk {
            session_id: self.session_id,
            first_frame,
            samples: Arc::new(std::mem::take(&mut self.interleave_buf)),
        };
        if self.tx.try_send(event).is_err() {
            self.dropped_chunks += 1;
            if self.dropped_chunks.is_power_of_two() {
                warn!(
                    "Multiroom tee channel full, dropped {} chunk(s) — followers will hear a gap",
                    self.dropped_chunks
                );
            }
        }
    }

    /// Compares where the leader's output actually is (`now + playback lag`
    /// for the last teed frame) against the nominal timeline and publishes
    /// the difference every ~2s. Called from the decode loop with the lag
    /// reported by the local audio output.
    ///
    /// The per-call offset is averaged over the interval: without driver
    /// timestamps the lag is ring-fill only, which sawtooths by a whole
    /// device buffer between callbacks — the average is the true center.
    pub fn maybe_send_correction(&mut self, playback_lag_micros: u64) {
        const INTERVAL_MICROS: u64 = 2_000_000;
        let now = MonoClock::now_micros();
        let nominal = self.epoch_micros + self.frames_sent * 1_000_000 / u64::from(self.rate.max(1));
        let actual = now + playback_lag_micros;
        #[allow(clippy::cast_possible_wrap)]
        let offset_micros = actual as i64 - nominal as i64;
        self.offset_sum += offset_micros;
        self.offset_samples += 1;

        if now.saturating_sub(self.last_correction_at) < INTERVAL_MICROS {
            return;
        }
        self.last_correction_at = now;
        let avg = self.offset_sum / i64::from(self.offset_samples.max(1));
        self.offset_sum = 0;
        self.offset_samples = 0;
        let _ = self.tx.try_send(TeeEvent::TimelineCorrection {
            session_id: self.session_id,
            offset_micros: avg,
        });
    }
}

impl Drop for TeeSession {
    fn drop(&mut self) {
        let _ = self.tx.try_send(TeeEvent::SessionEnd {
            session_id: self.session_id,
        });
    }
}
