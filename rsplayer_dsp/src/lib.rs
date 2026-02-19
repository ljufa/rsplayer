use anyhow::Result;
use camilladsp::basicfilters::Gain;
use camilladsp::biquad::Biquad;
use camilladsp::biquad::BiquadCoefficients;
pub use camilladsp::config::{self, BiquadParameters};
use camilladsp::filters::Filter;
use log::error;

pub struct Equalizer {
    channels: usize,
    // We use Box<dyn Filter + Send> to allow storing different filter types if needed,
    // and Send to allow moving between threads.
    filters: Vec<Vec<Box<dyn Filter + Send>>>,
    scratch_buffers: Vec<Vec<f32>>,
}

impl Equalizer {
    pub fn new(channels: usize) -> Self {
        let mut filters = Vec::with_capacity(channels);
        let mut scratch_buffers = Vec::with_capacity(channels);
        for _ in 0..channels {
            filters.push(Vec::new());
            scratch_buffers.push(Vec::new());
        }
        Self {
            channels,
            filters,
            scratch_buffers,
        }
    }

    pub fn add_biquad_filter(&mut self, channel: usize, samplerate: usize, params: BiquadParameters) -> Result<()> {
        if channel >= self.channels {
            return Err(anyhow::anyhow!("Channel index out of bounds"));
        }
        let coeffs = BiquadCoefficients::from_config(samplerate, params);
        let filter = Biquad::new("eq_band", samplerate, coeffs);
        self.filters[channel].push(Box::new(filter));
        Ok(())
    }

    // Helper to add the same filter to all channels (e.g. for room correction or bass boost)
    pub fn add_global_biquad_filter(&mut self, samplerate: usize, params: BiquadParameters) -> Result<()> {
        for ch in 0..self.channels {
            self.add_biquad_filter(ch, samplerate, params.clone())?;
        }
        Ok(())
    }

    pub fn add_gain_filter(&mut self, channel: usize, gain: f64) -> Result<()> {
        if channel >= self.channels {
            return Err(anyhow::anyhow!("Channel index out of bounds"));
        }
        let filter = Gain::new("gain_filter", gain as f32, false, false, false);
        self.filters[channel].push(Box::new(filter));
        Ok(())
    }

    pub fn add_global_gain_filter(&mut self, gain: f64) -> Result<()> {
        for ch in 0..self.channels {
            self.add_gain_filter(ch, gain)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        for ch in 0..self.channels {
            self.filters[ch].clear();
        }
    }

    pub fn process(&mut self, buffer: &mut [f32]) {
        if self.channels == 0 || buffer.is_empty() {
            return;
        }

        let frames = buffer.len() / self.channels;

        // Resize scratch buffers if needed
        for buf in &mut self.scratch_buffers {
            if buf.len() < frames {
                buf.resize(frames, 0.0);
            }
        }

        // De-interleave
        // buffer is interleaved [L, R, L, R, ...]
        for (i, chunk) in buffer.chunks(self.channels).enumerate() {
            for (ch, sample) in chunk.iter().enumerate() {
                if ch < self.channels {
                    self.scratch_buffers[ch][i] = *sample;
                }
            }
        }

        // Process
        for ch in 0..self.channels {
            // Only process if there are filters
            if !self.filters[ch].is_empty() {
                let channel_buffer = &mut self.scratch_buffers[ch][0..frames];
                for filter in &mut self.filters[ch] {
                    // process_waveform updates the buffer in place
                    if let Err(e) = filter.process_waveform(channel_buffer) {
                        error!("Filter processing error: {e}");
                    }
                }
            }
        }

        // Re-interleave
        for (i, chunk) in buffer.chunks_mut(self.channels).enumerate() {
            for (ch, sample) in chunk.iter_mut().enumerate() {
                if ch < self.channels {
                    *sample = self.scratch_buffers[ch][i];
                }
            }
        }
    }
}
