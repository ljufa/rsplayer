use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use api_models::settings::{DspFilter, DspSettings};
use rsplayer_dsp::{BiquadParameters, Equalizer};

/// Cross-thread DSP coordination — no `Equalizer` lives here.
///
/// The playback thread owns its `Equalizer` exclusively (stored in
/// `CpalAudioOutputImpl`).  When settings change from outside, a freshly-built
/// replacement is placed in `pending` and the `has_filters` flag is updated
/// atomically.  The playback thread swaps the pending equalizer in at the
/// start of `write()` via a cheap `try_lock` — no lock is ever held during
/// the actual DSP processing.
pub struct SharedDspState {
    pub dsp_settings: DspSettings,
    /// A freshly-built `Equalizer` waiting to be picked up by the playback
    /// thread.  `None` when no update is pending.
    pub pending: Mutex<Option<Equalizer>>,
    /// Lock-free flag checked at the top of every `write()` call.  `true`
    /// when the current equalizer (playback-thread-owned) has active filters.
    /// Updated with `Release` ordering so it is always consistent with the
    /// equalizer the playback thread holds.
    pub has_filters: Arc<AtomicBool>,
    /// Number of audio channels the equalizer was built for.
    pub channels: usize,
    /// Sample rate the equalizer was built for.
    pub rate: usize,
}

impl SharedDspState {
    pub fn new(dsp_settings: DspSettings) -> Self {
        Self {
            dsp_settings,
            pending: Mutex::new(None),
            has_filters: Arc::new(AtomicBool::new(false)),
            channels: 0,
            rate: 0,
        }
    }

    /// Apply `self.dsp_settings` to `eq` at `self.rate`.
    fn apply_filters(&self, eq: &mut Equalizer) {
        for filter_config in &self.dsp_settings.filters {
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
                        DspFilter::Peaking { freq, q, gain } => {
                            BiquadParameters::Peaking(rsplayer_dsp::config::PeakingWidth::Q {
                                freq: *freq as f32,
                                q: *q as f32,
                                gain: *gain as f32,
                            })
                        }
                        DspFilter::LowShelf { freq, q, slope, gain } => {
                            if let Some(s) = slope {
                                BiquadParameters::Lowshelf(rsplayer_dsp::config::ShelfSteepness::Slope {
                                    freq: *freq as f32,
                                    slope: *s as f32,
                                    gain: *gain as f32,
                                })
                            } else {
                                BiquadParameters::Lowshelf(rsplayer_dsp::config::ShelfSteepness::Q {
                                    freq: *freq as f32,
                                    q: q.map(|v| v as f32).unwrap_or(0.707),
                                    gain: *gain as f32,
                                })
                            }
                        }
                        DspFilter::HighShelf { freq, q, slope, gain } => {
                            if let Some(s) = slope {
                                BiquadParameters::Highshelf(rsplayer_dsp::config::ShelfSteepness::Slope {
                                    freq: *freq as f32,
                                    slope: *s as f32,
                                    gain: *gain as f32,
                                })
                            } else {
                                BiquadParameters::Highshelf(rsplayer_dsp::config::ShelfSteepness::Q {
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
                        DspFilter::Gain { .. } => unreachable!(),
                    };

                    if filter_config.channels.is_empty() {
                        if let Err(e) = eq.add_global_biquad_filter(self.rate, params) {
                            log::warn!("Failed to add equalizer filter: {e}");
                        }
                    } else {
                        for &ch in &filter_config.channels {
                            if let Err(e) = eq.add_biquad_filter(ch, self.rate, params.clone()) {
                                log::warn!("Failed to add equalizer filter for channel {ch}: {e}");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Build a fresh equalizer for the given audio spec and push it into the
    /// pending slot.  Called once per track when the audio output opens —
    /// never from the playback hot path.
    pub fn rebuild(&mut self, channels: usize, rate: usize) {
        self.channels = channels;
        self.rate = rate;
        let mut eq = Equalizer::new(channels);
        self.apply_filters(&mut eq);
        let active = eq.has_filters();
        // Store the new equalizer in the pending slot; the playback thread
        // will swap it in on the next write().
        if let Ok(mut slot) = self.pending.lock() {
            *slot = Some(eq);
        }
        self.has_filters.store(active, Ordering::Release);
    }

    /// Replace DSP settings, build a new equalizer, and push it to pending.
    /// Called from the command-handler thread — never from the playback thread.
    pub fn update_settings(&mut self, dsp_settings: DspSettings) {
        self.dsp_settings = dsp_settings;
        if self.rate > 0 && self.channels > 0 {
            let mut eq = Equalizer::new(self.channels);
            self.apply_filters(&mut eq);
            let active = eq.has_filters();
            if let Ok(mut slot) = self.pending.lock() {
                *slot = Some(eq);
            }
            self.has_filters.store(active, Ordering::Release);
        } else {
            // No track open yet — the flag stays false until rebuild() is called.
            self.has_filters.store(false, Ordering::Release);
        }
    }
}
