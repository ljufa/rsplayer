use std::f32::consts::PI;

// ── Public configuration types ──────────────────────────────────────────────

pub mod config {
    #[derive(Clone)]
    pub enum PeakingWidth {
        Q { freq: f32, q: f32, gain: f32 },
    }

    #[derive(Clone)]
    pub enum ShelfSteepness {
        Q { freq: f32, q: f32, gain: f32 },
        Slope { freq: f32, slope: f32, gain: f32 },
    }

    #[derive(Clone)]
    pub enum NotchWidth {
        Q { freq: f32, q: f32 },
    }
}

#[derive(Clone)]
pub enum BiquadParameters {
    Peaking(config::PeakingWidth),
    Lowshelf(config::ShelfSteepness),
    Highshelf(config::ShelfSteepness),
    Lowpass {
        freq: f32,
        q: f32,
    },
    Highpass {
        freq: f32,
        q: f32,
    },
    Bandpass(config::NotchWidth),
    Notch(config::NotchWidth),
    Allpass(config::NotchWidth),
    LowpassFO {
        freq: f32,
    },
    HighpassFO {
        freq: f32,
    },
    LowshelfFO {
        freq: f32,
        gain: f32,
    },
    HighshelfFO {
        freq: f32,
        gain: f32,
    },
    LinkwitzTransform {
        freq_act: f32,
        q_act: f32,
        freq_target: f32,
        q_target: f32,
    },
}

// ── Filter trait ─────────────────────────────────────────────────────────────

pub trait Filter: Send {
    fn process_waveform(&mut self, buf: &mut [f32]) -> anyhow::Result<()>;
}

// ── Biquad coefficients (normalized, a0 = 1) ─────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefficients {
    fn normalize(a0: f32, a1: f32, a2: f32, b0: f32, b1: f32, b2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Compute biquad coefficients from filter parameters.
    /// Formulas from the Audio EQ Cookbook (R. Bristow-Johnson) and `CamillaDSP`.
    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    pub fn from_config(fs: usize, params: &BiquadParameters) -> Self {
        let fs = fs as f32;
        match params {
            BiquadParameters::Highpass { freq, q } => {
                let w = 2.0 * PI * freq / fs;
                let cs = w.cos();
                let alpha = w.sin() / (2.0 * q);
                Self::normalize(
                    1.0 + alpha,
                    -2.0 * cs,
                    1.0 - alpha,
                    f32::midpoint(1.0, cs),
                    -(1.0 + cs),
                    f32::midpoint(1.0, cs),
                )
            }
            BiquadParameters::Lowpass { freq, q } => {
                let w = 2.0 * PI * freq / fs;
                let cs = w.cos();
                let alpha = w.sin() / (2.0 * q);
                Self::normalize(1.0 + alpha, -2.0 * cs, 1.0 - alpha, (1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0)
            }
            BiquadParameters::Bandpass(config::NotchWidth::Q { freq, q }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let alpha = sn / (2.0 * q);
                Self::normalize(1.0 + alpha, -2.0 * cs, 1.0 - alpha, alpha, 0.0, -alpha)
            }
            BiquadParameters::Notch(config::NotchWidth::Q { freq, q }) => {
                let w = 2.0 * PI * freq / fs;
                let cs = w.cos();
                let alpha = w.sin() / (2.0 * q);
                Self::normalize(1.0 + alpha, -2.0 * cs, 1.0 - alpha, 1.0, -2.0 * cs, 1.0)
            }
            BiquadParameters::Allpass(config::NotchWidth::Q { freq, q }) => {
                let w = 2.0 * PI * freq / fs;
                let cs = w.cos();
                let alpha = w.sin() / (2.0 * q);
                Self::normalize(1.0 + alpha, -2.0 * cs, 1.0 - alpha, 1.0 - alpha, -2.0 * cs, 1.0 + alpha)
            }
            BiquadParameters::Peaking(config::PeakingWidth::Q { freq, q, gain }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let a = 10.0_f32.powf(gain / 40.0);
                let alpha = sn / (2.0 * q);
                Self::normalize(
                    1.0 + alpha / a,
                    -2.0 * cs,
                    1.0 - alpha / a,
                    1.0 + alpha * a,
                    -2.0 * cs,
                    1.0 - alpha * a,
                )
            }
            BiquadParameters::Highshelf(config::ShelfSteepness::Q { freq, q, gain }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let a = 10.0_f32.powf(gain / 40.0);
                let beta = sn * a.sqrt() / q;
                Self::normalize(
                    (a - 1.0).mul_add(-cs, a + 1.0) + beta,
                    2.0 * (a + 1.0).mul_add(-cs, a - 1.0),
                    (a - 1.0).mul_add(-cs, a + 1.0) - beta,
                    a * ((a - 1.0).mul_add(cs, a + 1.0) + beta),
                    -2.0 * a * (a + 1.0).mul_add(cs, a - 1.0),
                    a * ((a - 1.0).mul_add(cs, a + 1.0) - beta),
                )
            }
            BiquadParameters::Highshelf(config::ShelfSteepness::Slope { freq, slope, gain }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let a = 10.0_f32.powf(gain / 40.0);
                let alpha = sn / 2.0 * (a + 1.0 / a).mul_add(1.0 / (slope / 12.0) - 1.0, 2.0).sqrt();
                let beta = 2.0 * a.sqrt() * alpha;
                Self::normalize(
                    (a - 1.0).mul_add(-cs, a + 1.0) + beta,
                    2.0 * (a + 1.0).mul_add(-cs, a - 1.0),
                    (a - 1.0).mul_add(-cs, a + 1.0) - beta,
                    a * ((a - 1.0).mul_add(cs, a + 1.0) + beta),
                    -2.0 * a * (a + 1.0).mul_add(cs, a - 1.0),
                    a * ((a - 1.0).mul_add(cs, a + 1.0) - beta),
                )
            }
            BiquadParameters::Lowshelf(config::ShelfSteepness::Q { freq, q, gain }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let a = 10.0_f32.powf(gain / 40.0);
                let beta = sn * a.sqrt() / q;
                Self::normalize(
                    (a - 1.0).mul_add(cs, a + 1.0) + beta,
                    -2.0 * (a + 1.0).mul_add(cs, a - 1.0),
                    (a - 1.0).mul_add(cs, a + 1.0) - beta,
                    a * ((a - 1.0).mul_add(-cs, a + 1.0) + beta),
                    2.0 * a * (a + 1.0).mul_add(-cs, a - 1.0),
                    a * ((a - 1.0).mul_add(-cs, a + 1.0) - beta),
                )
            }
            BiquadParameters::Lowshelf(config::ShelfSteepness::Slope { freq, slope, gain }) => {
                let w = 2.0 * PI * freq / fs;
                let sn = w.sin();
                let cs = w.cos();
                let a = 10.0_f32.powf(gain / 40.0);
                let alpha = sn / 2.0 * (a + 1.0 / a).mul_add(1.0 / (slope / 12.0) - 1.0, 2.0).sqrt();
                let beta = 2.0 * a.sqrt() * alpha;
                Self::normalize(
                    (a - 1.0).mul_add(cs, a + 1.0) + beta,
                    -2.0 * (a + 1.0).mul_add(cs, a - 1.0),
                    (a - 1.0).mul_add(cs, a + 1.0) - beta,
                    a * ((a - 1.0).mul_add(-cs, a + 1.0) + beta),
                    2.0 * a * (a + 1.0).mul_add(-cs, a - 1.0),
                    a * ((a - 1.0).mul_add(-cs, a + 1.0) - beta),
                )
            }
            BiquadParameters::LowpassFO { freq } => {
                let w = 2.0 * PI * freq / fs;
                let k = (w / 2.0).tan();
                let d = 1.0 + k;
                Self::normalize(1.0, -(1.0 - k) / d, 0.0, k / d, k / d, 0.0)
            }
            BiquadParameters::HighpassFO { freq } => {
                let w = 2.0 * PI * freq / fs;
                let k = (w / 2.0).tan();
                let d = 1.0 + k;
                Self::normalize(1.0, -(1.0 - k) / d, 0.0, 1.0 / d, -1.0 / d, 0.0)
            }
            BiquadParameters::LowshelfFO { freq, gain } => {
                let w = 2.0 * PI * freq / fs;
                let tn = (w / 2.0).tan();
                let a = 10.0_f32.powf(gain / 40.0);
                Self::normalize(tn + a, tn - a, 0.0, (a * a).mul_add(tn, a), (a * a).mul_add(tn, -a), 0.0)
            }
            BiquadParameters::HighshelfFO { freq, gain } => {
                let w = 2.0 * PI * freq / fs;
                let tn = (w / 2.0).tan();
                let a = 10.0_f32.powf(gain / 40.0);
                Self::normalize(a * tn + 1.0, a * tn - 1.0, 0.0, a.mul_add(tn, a * a), a.mul_add(tn, -a * a), 0.0)
            }
            BiquadParameters::LinkwitzTransform {
                freq_act,
                q_act,
                freq_target,
                q_target,
            } => {
                let d0i = (2.0 * PI * freq_act).powi(2);
                let d1i = (2.0 * PI * freq_act) / q_act;
                let c0i = (2.0 * PI * freq_target).powi(2);
                let c1i = (2.0 * PI * freq_target) / q_target;
                let fc = f32::midpoint(*freq_target, *freq_act);
                let gn = 2.0 * PI * fc / (PI * fc / fs).tan();
                let gn2 = gn.powi(2);
                let cci = c0i + gn * c1i + gn2;
                Self::normalize(
                    1.0,
                    2.0 * (c0i - gn2) / cci,
                    (c0i - gn * c1i + gn2) / cci,
                    (d0i + gn * d1i + gn2) / cci,
                    2.0 * (d0i - gn2) / cci,
                    (d0i - gn * d1i + gn2) / cci,
                )
            }
        }
    }
}

// ── Biquad filter: Direct Form 2 Transposed ───────────────────────────────────

pub struct Biquad {
    s1: f32,
    s2: f32,
    coeffs: BiquadCoefficients,
}

impl Biquad {
    pub const fn new(_name: &str, samplerate: usize, coefficients: BiquadCoefficients) -> Self {
        let _ = samplerate;
        Self {
            s1: 0.0,
            s2: 0.0,
            coeffs: coefficients,
        }
    }

    #[inline]
    fn process_single(&mut self, x: f32) -> f32 {
        let y = self.coeffs.b0.mul_add(x, self.s1);
        self.s1 = self.coeffs.a1.mul_add(-y, self.coeffs.b1.mul_add(x, self.s2));
        self.s2 = self.coeffs.b2.mul_add(x, -(self.coeffs.a2 * y));
        y
    }
}

impl Filter for Biquad {
    fn process_waveform(&mut self, buf: &mut [f32]) -> anyhow::Result<()> {
        for s in buf.iter_mut() {
            *s = self.process_single(*s);
        }
        // Flush subnormals to prevent denormal-number CPU slowdowns.
        if self.s1.is_subnormal() {
            self.s1 = 0.0;
        }
        if self.s2.is_subnormal() {
            self.s2 = 0.0;
        }
        Ok(())
    }
}

// ── Gain filter ───────────────────────────────────────────────────────────────

pub struct Gain {
    linear_gain: f32,
}

impl Gain {
    /// `gain_db` in dB; `inverted`, `mute`, `linear` match `CamillaDSP`'s `Gain::new` signature.
    pub fn new(_name: &str, gain_db: f32, inverted: bool, mute: bool, linear: bool) -> Self {
        let mut g = if linear { gain_db } else { 10.0_f32.powf(gain_db / 20.0) };
        if inverted {
            g = -g;
        }
        if mute {
            g = 0.0;
        }
        Self { linear_gain: g }
    }
}

impl Filter for Gain {
    fn process_waveform(&mut self, buf: &mut [f32]) -> anyhow::Result<()> {
        for s in buf.iter_mut() {
            *s *= self.linear_gain;
        }
        Ok(())
    }
}
