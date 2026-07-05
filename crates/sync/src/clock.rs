//! Clock synchronization between leader and follower.
//!
//! The follower sends `ClockMsg::Ping` datagrams; the leader answers with
//! `Pong { t1, t2, t3 }` and the follower stamps `t4` on receipt. From each
//! exchange we get an NTP-style offset/RTT sample:
//!
//! ```text
//! offset = ((t2 - t1) + (t3 - t4)) / 2      (leader clock - follower clock)
//! rtt    = (t4 - t1) - (t3 - t2)
//! ```
//!
//! Low-RTT samples carry the least asymmetry error, so the estimator keeps a
//! sliding window, takes the median offset of the lowest-RTT samples and
//! smooths it with an EWMA.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};

/// The process-wide monotonic clock, shared with the playback tee so leader
/// timestamps and clock probes use the same epoch.
pub use playback::rsp::tee::MonoClock;

/// Number of exchange samples kept in the sliding window.
const WINDOW: usize = 30;
/// How many of the lowest-RTT samples feed the median.
const BEST_OF: usize = 5;
/// Samples required before the offset is considered usable.
const MIN_SAMPLES: usize = 5;
/// EWMA smoothing factor applied to successive median estimates.
const EWMA_ALPHA: f64 = 0.1;

/// Shared, lock-free view of the current clock relation to the leader.
#[derive(Debug, Default)]
pub struct ClockState {
    /// `leader_clock - follower_clock` in µs (may be negative).
    offset_micros: AtomicI64,
    rtt_micros: AtomicU32,
    synced: AtomicBool,
}

impl ClockState {
    /// Leader-time → local-time conversion for scheduled playback.
    #[must_use]
    pub fn leader_to_local_micros(&self, leader_micros: u64) -> u64 {
        let offset = self.offset_micros.load(Ordering::Acquire);
        leader_micros.saturating_add_signed(-offset)
    }

    #[must_use]
    pub fn offset_micros(&self) -> i64 {
        self.offset_micros.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn rtt_micros(&self) -> u32 {
        self.rtt_micros.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn is_synced(&self) -> bool {
        self.synced.load(Ordering::Acquire)
    }
}

/// One completed ping/pong exchange, all times in µs on the respective
/// monotonic clocks.
#[derive(Debug, Clone, Copy)]
pub struct ExchangeSample {
    pub t1: u64,
    pub t2: u64,
    pub t3: u64,
    pub t4: u64,
}

impl ExchangeSample {
    #[must_use]
    fn offset_micros(&self) -> i64 {
        let out = i64::try_from(self.t2).unwrap_or(i64::MAX) - i64::try_from(self.t1).unwrap_or(i64::MAX);
        let back = i64::try_from(self.t3).unwrap_or(i64::MAX) - i64::try_from(self.t4).unwrap_or(i64::MAX);
        i64::midpoint(out, back)
    }

    #[must_use]
    fn rtt_micros(&self) -> i64 {
        let total = i64::try_from(self.t4).unwrap_or(i64::MAX) - i64::try_from(self.t1).unwrap_or(i64::MAX);
        let remote = i64::try_from(self.t3).unwrap_or(i64::MAX) - i64::try_from(self.t2).unwrap_or(i64::MAX);
        total - remote
    }
}

/// Offset estimator fed by the follower's clock task; publishes into a
/// shared [`ClockState`].
pub struct OffsetEstimator {
    window: Vec<(i64, i64)>, // (rtt, offset), insertion order
    smoothed: Option<f64>,
}

impl Default for OffsetEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl OffsetEstimator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            window: Vec::with_capacity(WINDOW),
            smoothed: None,
        }
    }

    /// Ingests one exchange and updates `state`.
    pub fn add_sample(&mut self, sample: ExchangeSample, state: &ClockState) {
        let rtt = sample.rtt_micros();
        if rtt < 0 {
            return; // nonsensical exchange (reordered datagrams)
        }
        if self.window.len() == WINDOW {
            self.window.remove(0);
        }
        self.window.push((rtt, sample.offset_micros()));

        if self.window.len() < MIN_SAMPLES {
            return;
        }

        let mut by_rtt = self.window.clone();
        by_rtt.sort_unstable();
        let best = &by_rtt[..BEST_OF.min(by_rtt.len())];
        let mut offsets: Vec<i64> = best.iter().map(|(_, o)| *o).collect();
        offsets.sort_unstable();
        #[allow(clippy::cast_precision_loss)]
        let median = offsets[offsets.len() / 2] as f64;

        let smoothed = self.smoothed.map_or(median, |prev| EWMA_ALPHA.mul_add(median - prev, prev));
        self.smoothed = Some(smoothed);

        #[allow(clippy::cast_possible_truncation)]
        state.offset_micros.store(smoothed.round() as i64, Ordering::Release);
        let best_rtt = by_rtt.first().map_or(0, |(r, _)| *r);
        state.rtt_micros.store(u32::try_from(best_rtt).unwrap_or(u32::MAX), Ordering::Release);
        state.synced.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a sample for a link with the given one-way delays and a true
    /// clock offset (leader ahead of follower by `true_offset`).
    fn sample(t1: u64, delay_out: u64, remote_processing: u64, delay_back: u64, true_offset: i64) -> ExchangeSample {
        let t2 = t1.saturating_add_signed(true_offset) + delay_out;
        let t3 = t2 + remote_processing;
        let t4 = t3.saturating_add_signed(-true_offset) + delay_back;
        ExchangeSample { t1, t2, t3, t4 }
    }

    #[test]
    fn symmetric_link_recovers_exact_offset() {
        let state = ClockState::default();
        let mut est = OffsetEstimator::new();
        for i in 0..10 {
            est.add_sample(sample(i * 100_000, 500, 20, 500, 250_000), &state);
        }
        assert!(state.is_synced());
        assert_eq!(state.offset_micros(), 250_000);
        assert_eq!(state.rtt_micros(), 1000);
    }

    #[test]
    fn negative_offset_supported() {
        let state = ClockState::default();
        let mut est = OffsetEstimator::new();
        for i in 0..10 {
            est.add_sample(sample(10_000_000 + i * 100_000, 300, 10, 300, -42_000), &state);
        }
        assert_eq!(state.offset_micros(), -42_000);
        assert_eq!(state.leader_to_local_micros(1_000_000), 1_042_000);
    }

    #[test]
    fn not_synced_before_min_samples() {
        let state = ClockState::default();
        let mut est = OffsetEstimator::new();
        for i in 0..(MIN_SAMPLES as u64 - 1) {
            est.add_sample(sample(i * 100_000, 500, 20, 500, 1000), &state);
        }
        assert!(!state.is_synced());
        est.add_sample(sample(1_000_000, 500, 20, 500, 1000), &state);
        assert!(state.is_synced());
    }

    #[test]
    fn asymmetric_jitter_filtered_by_low_rtt_median() {
        let state = ClockState::default();
        let mut est = OffsetEstimator::new();
        // Mostly clean samples with true offset 100_000...
        for i in 0..20 {
            est.add_sample(sample(i * 100_000, 400, 10, 400, 100_000), &state);
        }
        // ...plus wildly asymmetric (bufferbloat-style) outliers.
        for i in 20..25 {
            est.add_sample(sample(i * 100_000, 30_000, 10, 200, 100_000), &state);
        }
        // Outliers have high RTT, so the best-of-RTT median ignores them.
        let err = (state.offset_micros() - 100_000).abs();
        assert!(err < 100, "offset error too large: {err}µs");
    }

    #[test]
    fn reordered_exchange_rejected() {
        let state = ClockState::default();
        let mut est = OffsetEstimator::new();
        // t4 < t1 would produce a negative RTT.
        est.add_sample(
            ExchangeSample {
                t1: 1000,
                t2: 2000,
                t3: 2100,
                t4: 900,
            },
            &state,
        );
        assert!(!state.is_synced());
    }

    #[test]
    fn mono_clock_is_monotonic() {
        let a = MonoClock::now_micros();
        let b = MonoClock::now_micros();
        assert!(b >= a);
    }
}
