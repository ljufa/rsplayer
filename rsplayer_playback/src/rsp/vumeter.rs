use api_models::state::StateChangeEvent;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::conv::IntoSample;
use tokio::sync::broadcast::Sender;

const VU_UPDATE_INTERVAL: Duration = Duration::from_millis(50);

/// VU meter state and logic.
///
/// When VU metering is disabled the caller should simply not create a
/// `VUMeter` at all (use `Option<VUMeter>` in the owning struct).
pub(crate) struct VUMeter {
    /// Last time a VU event was sent to the frontend.
    last_update: std::time::Instant,
    /// Current maximum absolute sample value (left channel).
    current_max_l: f32,
    /// Current maximum absolute sample value (right channel).
    current_max_r: f32,
    /// Current volume level (0-255) used to scale peaks.
    volume: Arc<AtomicU8>,
    /// Channel to send VU events to the frontend.
    changes_tx: Sender<StateChangeEvent>,
}

impl VUMeter {
    pub(crate) fn new(volume: Arc<AtomicU8>, changes_tx: Sender<StateChangeEvent>) -> Self {
        Self {
            last_update: std::time::Instant::now(),
            current_max_l: 0.0,
            current_max_r: 0.0,
            volume,
            changes_tx,
        }
    }

    /// Update peak values from a slice of samples.
    /// `samples` is an interleaved slice of channel-count samples.
    pub(crate) fn update_peaks(&mut self, channels: usize, samples: &[impl IntoSample<f32> + Copy]) {
        let volume_factor = self.volume.load(Ordering::Relaxed) as f32 / 255.0;
        if channels >= 2 {
            for chunk in samples.chunks(channels) {
                if chunk.len() >= 2 {
                    let l_raw: f32 = chunk[0].into_sample();
                    let r_raw: f32 = chunk[1].into_sample();
                    let l = l_raw * volume_factor;
                    let r = r_raw * volume_factor;
                    if l.abs() > self.current_max_l {
                        self.current_max_l = l.abs();
                    }
                    if r.abs() > self.current_max_r {
                        self.current_max_r = r.abs();
                    }
                }
            }
        } else if channels == 1 {
            for s in samples {
                let v_raw: f32 = (*s).into_sample();
                let v = v_raw * volume_factor;
                if v.abs() > self.current_max_l {
                    self.current_max_l = v.abs();
                }
            }
            self.current_max_r = self.current_max_l;
        }
    }

    /// If enough time has passed since the last update, send a VU event
    /// and reset the peak values. Returns `true` if an event was sent.
    pub(crate) fn maybe_send_event(&mut self) -> bool {
        if self.last_update.elapsed() > VU_UPDATE_INTERVAL {
            let vu_l = (self.current_max_l * 255.0).min(255.0) as u8;
            let vu_r = (self.current_max_r * 255.0).min(255.0) as u8;
            _ = self.changes_tx.send(StateChangeEvent::VUEvent(vu_l, vu_r));
            self.last_update = std::time::Instant::now();
            self.current_max_l = 0.0;
            self.current_max_r = 0.0;
            true
        } else {
            false
        }
    }
}
