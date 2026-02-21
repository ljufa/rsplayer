use log::{info, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use crate::{BiquadParameters, Equalizer};
use api_models::settings::{DspFilter, DspSettings};

/// Apply DSP settings filters to `eq` for the given `rate`.
fn apply_filters_with_settings(dsp_settings: &DspSettings, eq: &mut Equalizer, rate: usize) {
    for filter_config in &dsp_settings.filters {
        match &filter_config.filter {
            DspFilter::Gain { gain } => {
                if filter_config.channels.is_empty() {
                    if let Err(e) = eq.add_global_gain_filter(*gain) {
                        log::warn!("Failed to add global gain filter: {e}");
                    }
                } else {
                    for &ch in &filter_config.channels {
                        if let Err(e) = eq.add_gain_filter(ch, *gain) {
                            log::warn!("Failed to add gain filter for channel {ch}: {e}");
                        }
                    }
                }
            }
            other_filter => {
                let params = match other_filter {
                    DspFilter::Peaking { freq, q, gain } => BiquadParameters::Peaking(crate::config::PeakingWidth::Q {
                        freq: *freq as f32,
                        q: *q as f32,
                        gain: *gain as f32,
                    }),
                    DspFilter::LowShelf { freq, q, slope, gain } => {
                        if let Some(s) = slope {
                            BiquadParameters::Lowshelf(crate::config::ShelfSteepness::Slope {
                                freq: *freq as f32,
                                slope: *s as f32,
                                gain: *gain as f32,
                            })
                        } else {
                            BiquadParameters::Lowshelf(crate::config::ShelfSteepness::Q {
                                freq: *freq as f32,
                                q: q.map(|v| v as f32).unwrap_or(0.707),
                                gain: *gain as f32,
                            })
                        }
                    }
                    DspFilter::HighShelf { freq, q, slope, gain } => {
                        if let Some(s) = slope {
                            BiquadParameters::Highshelf(crate::config::ShelfSteepness::Slope {
                                freq: *freq as f32,
                                slope: *s as f32,
                                gain: *gain as f32,
                            })
                        } else {
                            BiquadParameters::Highshelf(crate::config::ShelfSteepness::Q {
                                freq: *freq as f32,
                                q: q.map(|v| v as f32).unwrap_or(0.707),
                                gain: *gain as f32,
                            })
                        }
                    }
                    DspFilter::LowPass { freq, q } => BiquadParameters::Lowpass {
                        freq: *freq as f32,
                        q: *q as f32,
                    },
                    DspFilter::HighPass { freq, q } => BiquadParameters::Highpass {
                        freq: *freq as f32,
                        q: *q as f32,
                    },
                    DspFilter::BandPass { freq, q } => BiquadParameters::Bandpass(crate::config::NotchWidth::Q {
                        freq: *freq as f32,
                        q: *q as f32,
                    }),
                    DspFilter::Notch { freq, q } => BiquadParameters::Notch(crate::config::NotchWidth::Q {
                        freq: *freq as f32,
                        q: *q as f32,
                    }),
                    DspFilter::AllPass { freq, q } => BiquadParameters::Allpass(crate::config::NotchWidth::Q {
                        freq: *freq as f32,
                        q: *q as f32,
                    }),
                    DspFilter::LowPassFO { freq } => BiquadParameters::LowpassFO { freq: *freq as f32 },
                    DspFilter::HighPassFO { freq } => BiquadParameters::HighpassFO { freq: *freq as f32 },
                    DspFilter::LowShelfFO { freq, gain } => BiquadParameters::LowshelfFO {
                        freq: *freq as f32,
                        gain: *gain as f32,
                    },
                    DspFilter::HighShelfFO { freq, gain } => BiquadParameters::HighshelfFO {
                        freq: *freq as f32,
                        gain: *gain as f32,
                    },
                    DspFilter::LinkwitzTransform {
                        freq_act,
                        q_act,
                        freq_target,
                        q_target,
                    } => BiquadParameters::LinkwitzTransform {
                        freq_act: *freq_act as f32,
                        q_act: *q_act as f32,
                        freq_target: *freq_target as f32,
                        q_target: *q_target as f32,
                    },
                    DspFilter::Gain { .. } => unreachable!(),
                };

                if filter_config.channels.is_empty() {
                    if let Err(e) = eq.add_global_biquad_filter(rate, params) {
                        log::warn!("Failed to add equalizer filter: {e}");
                    }
                } else {
                    for &ch in &filter_config.channels {
                        if let Err(e) = eq.add_biquad_filter(ch, rate, params.clone()) {
                            log::warn!("Failed to add equalizer filter for channel {ch}: {e}");
                        }
                    }
                }
            }
        }
    }
}

/// The shared state that crosses the thread boundary between the
/// command-handler and the playback thread.  The playback thread holds a
/// `DspHandle`; the command-handler thread owns the `DspProcessor`
/// exclusively — no outer `Mutex<DspProcessor>` is needed.
///
/// All fields are `Arc`-wrapped so cloning the handle is cheap and neither
/// side needs to lock the other's data during audio processing.
#[derive(Clone)]
pub struct DspHandle {
    /// A freshly-built `Equalizer` waiting to be picked up by the playback
    /// thread.  `None` when no update is pending.
    pub pending: Arc<Mutex<Option<Equalizer>>>,
    /// Lock-free flag checked at the top of every `write()` call.  `true`
    /// when the current equalizer has active filters.
    pub has_filters: Arc<AtomicBool>,
    /// Current DSP settings — shared so `update_settings` on the command
    /// thread is immediately visible to the playback thread's `rebuild`.
    settings: Arc<Mutex<DspSettings>>,
}

impl DspHandle {
    /// Build a fresh equalizer for `channels`/`rate` and push it into the
    /// pending slot.  Called by `AlsaOutput::new` on the playback thread
    /// when a new track opens — no `DspProcessor` reference is needed.
    pub fn rebuild(&self, channels: usize, rate: usize) {
        if let Ok(settings) = self.settings.lock() {
            let mut eq = Equalizer::new(channels);
            apply_filters_with_settings(&settings, &mut eq, rate);
            let active = eq.has_filters();
            if let Ok(mut slot) = self.pending.lock() {
                *slot = Some(eq);
            }
            self.has_filters.store(active, Ordering::Release);
        }
    }
}

/// Command-handler-side DSP owner.  Exclusively owned by `PlayerService` —
/// never wrapped in a `Mutex`.  Cross-thread communication with the playback
/// thread happens only through the `Arc`s inside the `DspHandle`.
pub struct DspProcessor {
    /// Number of audio channels the equalizer was last built for.
    pub channels: usize,
    /// Sample rate the equalizer was last built for.
    pub rate: usize,
    /// The shared state given to the playback thread.
    handle: DspHandle,
}

impl DspProcessor {
    pub fn new(dsp_settings: DspSettings) -> Self {
        Self {
            channels: 0,
            rate: 0,
            handle: DspHandle {
                pending: Arc::new(Mutex::new(None)),
                has_filters: Arc::new(AtomicBool::new(false)),
                settings: Arc::new(Mutex::new(dsp_settings)),
            },
        }
    }

    /// Return a `DspHandle` to pass to the playback thread.  Cloning the
    /// `Arc`s is the only cross-thread operation — no lock is taken here.
    pub fn handle(&self) -> DspHandle {
        DspHandle {
            pending: self.handle.pending.clone(),
            has_filters: self.handle.has_filters.clone(),
            settings: self.handle.settings.clone(),
        }
    }

    /// Replace DSP settings, build a new equalizer, and push it to pending.
    /// Called from the command-handler thread — exclusively owned, no mutex.
    pub fn update_settings(&mut self, dsp_settings: DspSettings) {
        if let Ok(mut s) = self.handle.settings.lock() {
            *s = dsp_settings.clone();
        }
        if self.rate > 0 && self.channels > 0 {
            info!(
                "Rebuilding equalizer with rate {} and channels {}",
                self.rate, self.channels
            );
            let mut eq = Equalizer::new(self.channels);
            apply_filters_with_settings(&dsp_settings, &mut eq, self.rate);
            let active = eq.has_filters();
            if let Ok(mut slot) = self.handle.pending.lock() {
                *slot = Some(eq);
            }
            self.handle.has_filters.store(active, Ordering::Release);
        } else {
            warn!(
                "Skipping equalizer rebuild: rate or channels is 0 (rate={}, channels={})",
                self.rate, self.channels
            );
            self.handle.has_filters.store(false, Ordering::Release);
        }
    }
}
