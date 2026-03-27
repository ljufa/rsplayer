use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::Arc;

use api_models::state::StateChangeEvent;
use rsplayer_dsp::DspHandle;
use tokio::sync::broadcast::Sender;

use crate::rsp::vumeter::VUMeter;

#[derive(Clone)]
pub struct PlaybackContext {
    pub stop_signal: Arc<AtomicBool>,
    pub skip_to_time: Arc<AtomicU16>,
    #[allow(dead_code)]
    pub current_volume: Arc<AtomicU8>,
    pub changes_tx: Sender<StateChangeEvent>,
    pub dsp_handle: Option<DspHandle>,
    pub vu_meter: Option<VUMeter>,
}

impl PlaybackContext {
    pub fn new(
        stop_signal: Arc<AtomicBool>,
        skip_to_time: Arc<AtomicU16>,
        current_volume: Arc<AtomicU8>,
        changes_tx: Sender<StateChangeEvent>,
        dsp_handle: Option<DspHandle>,
        vu_meter_enabled: bool,
    ) -> Self {
        let vu_meter = if vu_meter_enabled {
            Some(VUMeter::new(current_volume.clone(), changes_tx.clone()))
        } else {
            None
        };

        Self {
            stop_signal,
            skip_to_time,
            current_volume,
            changes_tx,
            dsp_handle,
            vu_meter,
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.stop_signal.load(Ordering::Relaxed)
    }

    pub fn consume_skip_time(&self) -> u16 {
        self.skip_to_time.swap(0, Ordering::Relaxed)
    }

    pub const fn take_vu_meter(&mut self) -> Option<VUMeter> {
        self.vu_meter.take()
    }
}
