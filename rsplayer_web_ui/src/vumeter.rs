use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VisualizerType {
    None,
    NeonBar,
    Spectrum,
    Wave,
    Circular,
    Lissajous,
    Particles,
    Mirror,
    Starfield,
    Dna,
    Plasma,
    Tunnel,
    Bounce,
}

impl VisualizerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NeonBar => "neonbar",
            Self::Spectrum => "spectrum",
            Self::Wave => "wave",
            Self::Circular => "circular",
            Self::Lissajous => "lissajous",
            Self::Particles => "particles",
            Self::Mirror => "mirror",
            Self::Starfield => "starfield",
            Self::Dna => "dna",
            Self::Plasma => "plasma",
            Self::Tunnel => "tunnel",
            Self::Bounce => "bounce",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "neonbar" => Some(Self::NeonBar),
            "spectrum" => Some(Self::Spectrum),
            "wave" => Some(Self::Wave),
            "circular" => Some(Self::Circular),
            "lissajous" => Some(Self::Lissajous),
            "particles" => Some(Self::Particles),
            "mirror" => Some(Self::Mirror),
            "starfield" => Some(Self::Starfield),
            "dna" => Some(Self::Dna),
            "plasma" => Some(Self::Plasma),
            "tunnel" => Some(Self::Tunnel),
            "bounce" => Some(Self::Bounce),
            _ => None,
        }
    }
}

/// Particle: [x_px, y_px, vx_px, vy_px, life (0–1)]
type Particle = [f64; 5];

pub struct VUMeter {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    left: u8,
    right: u8,
    smoothed_left: f64,
    smoothed_right: f64,
    visualizer_type: VisualizerType,
    spectrum_bars_left: Vec<f64>,
    spectrum_bars_right: Vec<f64>,
    wave_points: Vec<f64>,       // left channel
    wave_points_right: Vec<f64>, // right channel
    wave_phase: f64,
    circular_bars: Vec<f64>,
    lissajous_phase: f64,
    particles: Vec<Particle>,
    /// Pulse rings for the Mirror visualizer: [radius_norm, life, hue]
    pulse_rings: Vec<[f64; 3]>,
    /// Starfield: [x(-1..1), y(-1..1), z(0..1), prev_z]
    stars: Vec<[f64; 4]>,
    plasma_time: f64,
    /// Bounce balls: [x(0..1), y(0..1), vx, vy, hue]
    bounce_balls: Vec<[f64; 5]>,
    peak_left: f64,
    peak_right: f64,
    spectrum_peaks_left: Vec<f64>,
    spectrum_peaks_right: Vec<f64>,
}

impl VUMeter {

    pub fn with_type(canvas_id: &str, visualizer_type: VisualizerType) -> Option<Self> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let canvas = document.get_element_by_id(canvas_id)?;
        let canvas: HtmlCanvasElement = canvas.dyn_into().ok()?;

        let ctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;

        let bar_count = 24;
        let spectrum_bars_left = vec![0.0; bar_count];
        let spectrum_bars_right = vec![0.0; bar_count];
        let wave_point_count = 128;
        let wave_points = vec![0.0; wave_point_count];
        let circular_bar_count = 64;
        let circular_bars = vec![0.0; circular_bar_count];

        let mut meter = Self {
            canvas,
            ctx,
            left: 0,
            right: 0,
            smoothed_left: 0.0,
            smoothed_right: 0.0,
            visualizer_type,
            spectrum_bars_left,
            spectrum_bars_right,
            wave_points,
            wave_points_right: vec![0.0; wave_point_count],
            wave_phase: 0.0,
            circular_bars,
            lissajous_phase: 0.0,
            particles: Vec::new(),
            pulse_rings: Vec::new(),
            stars: Vec::new(),
            plasma_time: 0.0,
            bounce_balls: Vec::new(),
            peak_left: 0.0,
            peak_right: 0.0,
            spectrum_peaks_left: vec![0.0; bar_count],
            spectrum_peaks_right: vec![0.0; bar_count],
        };
        meter.resize();
        meter.draw();
        Some(meter)
    }

    pub fn resize(&mut self) {
        let rect = self.canvas.get_bounding_client_rect();
        self.canvas.set_width(rect.width() as u32);
        self.canvas.set_height(rect.height() as u32);
        self.draw();
    }

    pub fn update(&mut self, left: u8, right: u8) {
        self.left = left;
        self.right = right;
        let smoothing = 0.3;
        self.smoothed_left = self.smoothed_left + (left as f64 - self.smoothed_left) * smoothing;
        self.smoothed_right = self.smoothed_right + (right as f64 - self.smoothed_right) * smoothing;

        // Peak hold: snap up instantly, decay slowly
        if self.smoothed_left >= self.peak_left {
            self.peak_left = self.smoothed_left;
        } else {
            self.peak_left = (self.peak_left - 0.7).max(0.0);
        }
        if self.smoothed_right >= self.peak_right {
            self.peak_right = self.smoothed_right;
        } else {
            self.peak_right = (self.peak_right - 0.7).max(0.0);
        }

        match self.visualizer_type {
            VisualizerType::Spectrum => self.update_spectrum_bars(),
            VisualizerType::Wave => self.update_wave(),
            VisualizerType::Mirror => self.update_pulse_rings(),
            VisualizerType::Tunnel => self.update_pulse_rings(),
            VisualizerType::Starfield => self.update_starfield(),
            VisualizerType::Dna => self.update_dna(),
            VisualizerType::Plasma => self.update_plasma(),
            VisualizerType::Bounce => self.update_bounce(),
            VisualizerType::Circular => self.update_circular(),
            VisualizerType::Lissajous => self.update_lissajous(),
            VisualizerType::Particles => self.update_particles(),
            _ => {}
        }

        self.draw();
    }

    fn update_spectrum_bars(&mut self) {
        let base_left = self.smoothed_left;
        let base_right = self.smoothed_right;
        let bar_count = self.spectrum_bars_left.len();

        for i in 0..bar_count {
            let freq_factor = ((i as f64 / bar_count as f64) * 3.0).sin().abs() * 0.4 + 0.6;
            let random_factor = 0.7 + (js_sys::Math::random() * 0.3);
            let decay_factor = if base_left > 100.0 { 0.5 } else { 0.65 };

            let target_left = (base_left * freq_factor * random_factor).min(255.0);
            let target_right = (base_right * freq_factor * random_factor).min(255.0);

            self.spectrum_bars_left[i] = self.spectrum_bars_left[i] * decay_factor + target_left * (1.0 - decay_factor);
            self.spectrum_bars_right[i] =
                self.spectrum_bars_right[i] * decay_factor + target_right * (1.0 - decay_factor);

            // Peak hold per bar
            if self.spectrum_bars_left[i] >= self.spectrum_peaks_left[i] {
                self.spectrum_peaks_left[i] = self.spectrum_bars_left[i];
            } else {
                self.spectrum_peaks_left[i] = (self.spectrum_peaks_left[i] - 1.2).max(0.0);
            }
            if self.spectrum_bars_right[i] >= self.spectrum_peaks_right[i] {
                self.spectrum_peaks_right[i] = self.spectrum_bars_right[i];
            } else {
                self.spectrum_peaks_right[i] = (self.spectrum_peaks_right[i] - 1.2).max(0.0);
            }
        }
    }

    fn update_wave(&mut self) {
        let avg_l = self.smoothed_left;
        let avg_r = self.smoothed_right;
        self.wave_phase += 0.7 + (avg_l / 255.0) * 0.5;

        let point_count = self.wave_points.len();
        for i in 0..point_count {
            let t = i as f64 / point_count as f64;

            // Left channel: 3 cycles
            let amp_l = (avg_l / 255.0).max(0.05);
            let noise_l = (js_sys::Math::random() - 0.5) * 0.08 * amp_l;
            let target_l = (t * std::f64::consts::TAU * 3.0 + self.wave_phase).sin() * amp_l + noise_l;
            let decay_l = if avg_l > 50.0 { 0.25 } else { 0.5 };
            self.wave_points[i] = self.wave_points[i] * decay_l + target_l * (1.0 - decay_l);

            // Right channel: 5 cycles, desynchronised phase
            let amp_r = (avg_r / 255.0).max(0.05);
            let noise_r = (js_sys::Math::random() - 0.5) * 0.08 * amp_r;
            let target_r = (t * std::f64::consts::TAU * 5.0 + self.wave_phase * 0.71).sin() * amp_r + noise_r;
            let decay_r = if avg_r > 50.0 { 0.25 } else { 0.5 };
            self.wave_points_right[i] = self.wave_points_right[i] * decay_r + target_r * (1.0 - decay_r);
        }
    }

    fn update_circular(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let bar_count = self.circular_bars.len();

        for i in 0..bar_count {
            let t = i as f64 / bar_count as f64;
            let freq_factor = ((t * 4.0 + self.wave_phase * 0.5).sin() + 1.0) * 0.5;
            let random_factor = 0.6 + js_sys::Math::random() * 0.4;
            let target = (avg * freq_factor * random_factor).min(255.0);

            let decay = if avg > 50.0 { 0.4 } else { 0.65 };
            self.circular_bars[i] = self.circular_bars[i] * decay + target * (1.0 - decay);
        }

        self.wave_phase += 0.4;
    }

    fn update_lissajous(&mut self) {
        // Slowly rotate the figure so it never looks static
        self.lissajous_phase += 0.012;
        if self.lissajous_phase > std::f64::consts::TAU {
            self.lissajous_phase -= std::f64::consts::TAU;
        }
    }

    fn update_pulse_rings(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let intensity = avg / 255.0;

        // Spawn a ring every update; size and color reflect current volume
        if intensity > 0.02 {
            let hue = 220.0 - intensity * 220.0; // blue (quiet) → red (loud)
            self.pulse_rings.push([0.0, intensity.powf(0.6), hue]);
        }

        // Expand and fade each ring
        self.pulse_rings = self
            .pulse_rings
            .drain(..)
            .filter_map(|mut r| {
                r[0] += 0.07;
                r[1] -= 0.07;
                if r[1] > 0.01 { Some(r) } else { None }
            })
            .collect();

        if self.pulse_rings.len() > 30 {
            let excess = self.pulse_rings.len() - 30;
            self.pulse_rings.drain(0..excess);
        }
    }

    fn update_starfield(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let speed = 0.008 + (avg / 255.0) * 0.035;

        if self.stars.is_empty() {
            for _ in 0..200 {
                let z = js_sys::Math::random();
                self.stars.push([
                    js_sys::Math::random() * 2.0 - 1.0,
                    js_sys::Math::random() * 2.0 - 1.0,
                    z,
                    (z + 0.001).min(1.0),
                ]);
            }
        }

        for star in &mut self.stars {
            star[3] = star[2];
            star[2] -= speed;
            if star[2] <= 0.01 {
                star[0] = js_sys::Math::random() * 2.0 - 1.0;
                star[1] = js_sys::Math::random() * 2.0 - 1.0;
                star[2] = 1.0;
                star[3] = 1.0;
            }
        }
    }

    fn update_dna(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        self.wave_phase += 0.12 + (avg / 255.0) * 0.08;
    }

    fn update_plasma(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        self.plasma_time += 0.025 + (avg / 255.0) * 0.025;
    }

    fn update_bounce(&mut self) {
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let intensity = avg / 255.0;

        if self.bounce_balls.is_empty() {
            for i in 0..6 {
                let angle = (i as f64 / 6.0) * std::f64::consts::TAU;
                let speed = 0.004 + js_sys::Math::random() * 0.004;
                self.bounce_balls.push([
                    js_sys::Math::random(),
                    js_sys::Math::random(),
                    angle.cos() * speed,
                    angle.sin() * speed,
                    i as f64 / 6.0 * 360.0,
                ]);
            }
        }

        let speed_factor = 1.0 + intensity * 4.0;
        for ball in &mut self.bounce_balls {
            ball[0] += ball[2] * speed_factor;
            ball[1] += ball[3] * speed_factor;
            if ball[0] < 0.0 { ball[0] = 0.0; ball[2] = ball[2].abs(); }
            if ball[0] > 1.0 { ball[0] = 1.0; ball[2] = -ball[2].abs(); }
            if ball[1] < 0.0 { ball[1] = 0.0; ball[3] = ball[3].abs(); }
            if ball[1] > 1.0 { ball[1] = 1.0; ball[3] = -ball[3].abs(); }
            ball[4] = (ball[4] + 0.5 + intensity * 1.5) % 360.0;
        }
    }

    fn update_particles(&mut self) {
        let width = self.canvas.width() as f64;
        let height = self.canvas.height() as f64;
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let intensity = avg / 255.0;

        let cx = width / 2.0;
        let cy = height / 2.0;
        let aspect = height / width;

        // Spawn more particles when louder
        let spawn = (intensity * 5.0) as usize + if intensity > 0.05 { 1 } else { 0 };
        for _ in 0..spawn {
            let angle = js_sys::Math::random() * std::f64::consts::TAU;
            let speed = (1.5 + js_sys::Math::random() * 4.0) * intensity * (width * 0.012);
            self.particles.push([
                cx,
                cy,
                angle.cos() * speed,
                angle.sin() * speed * aspect, // scale so particles spread evenly on wide canvas
                1.0,
            ]);
        }

        // Advance and cull
        self.particles = self
            .particles
            .drain(..)
            .filter_map(|mut p| {
                p[0] += p[2];
                p[1] += p[3];
                p[4] -= 0.03;
                if p[4] > 0.0 && p[0] >= 0.0 && p[0] <= width && p[1] >= 0.0 && p[1] <= height {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();

        if self.particles.len() > 250 {
            let excess = self.particles.len() - 250;
            self.particles.drain(0..excess);
        }
    }

    fn draw(&self) {
        let width = self.canvas.width() as f64;
        let height = self.canvas.height() as f64;

        self.ctx.clear_rect(0.0, 0.0, width, height);

        match self.visualizer_type {
            VisualizerType::None => {}
            VisualizerType::NeonBar => self.draw_neon_bars(width, height),
            VisualizerType::Spectrum => self.draw_spectrum(width, height),
            VisualizerType::Wave => self.draw_wave(width, height),
            VisualizerType::Circular => self.draw_circular(width, height),
            VisualizerType::Lissajous => self.draw_lissajous(width, height),
            VisualizerType::Particles => self.draw_particles(width, height),
            VisualizerType::Mirror => self.draw_mirror(width, height),
            VisualizerType::Starfield => self.draw_starfield(width, height),
            VisualizerType::Dna => self.draw_dna(width, height),
            VisualizerType::Plasma => self.draw_plasma(width, height),
            VisualizerType::Tunnel => self.draw_tunnel(width, height),
            VisualizerType::Bounce => self.draw_bounce(width, height),
        }
    }

    fn draw_neon_bars(&self, width: f64, height: f64) {
        let padding = (height * 0.08).max(4.0);
        let gap = (height * 0.1).max(4.0);
        let bar_height = ((height - padding * 2.0 - gap) / 2.0).max(1.0);
        let y1 = padding;
        let y2 = padding + bar_height + gap;

        self.draw_neon_bar(0.0, y1, width, bar_height, self.smoothed_left, self.peak_left);
        self.draw_neon_bar(0.0, y2, width, bar_height, self.smoothed_right, self.peak_right);
    }

    fn draw_spectrum(&self, width: f64, height: f64) {
        let bar_count = self.spectrum_bars_left.len();
        let bar_count_f = bar_count as f64;
        let gap = 2.5;
        let bar_width = (width - gap * (bar_count_f - 1.0)) / bar_count_f;
        let mid_y = height / 2.0;
        let max_bar_h = mid_y - 6.0;

        self.ctx.clear_rect(0.0, 0.0, width, height);

        // Faint centre line
        self.ctx.set_stroke_style_str("rgba(255,255,255,0.06)");
        self.ctx.set_line_width(1.0);
        self.ctx.begin_path();
        self.ctx.move_to(0.0, mid_y);
        self.ctx.line_to(width, mid_y);
        self.ctx.stroke();

        for i in 0..bar_count {
            let x = i as f64 * (bar_width + gap);
            // Hue: violet (270°) at left → red (0°) at right
            let hue = 270.0 - (i as f64 / (bar_count_f - 1.0)) * 270.0;

            let h_left  = (self.spectrum_bars_left[i]  / 255.0) * max_bar_h;
            let h_right = (self.spectrum_bars_right[i] / 255.0) * max_bar_h;
            let ph_left  = (self.spectrum_peaks_left[i]  / 255.0) * max_bar_h;
            let ph_right = (self.spectrum_peaks_right[i] / 255.0) * max_bar_h;

            // Left channel — grows upward from centre
            self.draw_spectrum_bar(x, mid_y, bar_width, h_left, hue, true);
            // Right channel — grows downward from centre
            self.draw_spectrum_bar(x, mid_y, bar_width, h_right, hue, false);

            // Peak dots
            if ph_left > 2.0 {
                self.draw_spectrum_peak(x, mid_y - ph_left - 2.0, bar_width, hue);
            }
            if ph_right > 2.0 {
                self.draw_spectrum_peak(x, mid_y + ph_right + 1.0, bar_width, hue);
            }
        }
    }

    /// Draw one spectrum column; `base_y` is the centre line, `h` is bar height, `up` selects direction.
    fn draw_spectrum_bar(&self, x: f64, base_y: f64, w: f64, h: f64, hue: f64, up: bool) {
        if h < 1.0 {
            return;
        }
        let (y, grad_top, grad_bot) = if up {
            (base_y - h, base_y - h, base_y)
        } else {
            (base_y, base_y, base_y + h)
        };

        let color_tip  = format!("hsl({:.0}, 100%, 65%)", hue);
        let color_base = format!("hsla({:.0}, 100%, 40%, 0.55)", hue);

        let grad = self.ctx.create_linear_gradient(x, grad_top, x, grad_bot);
        if up {
            let _ = grad.add_color_stop(0.0, &color_tip);
            let _ = grad.add_color_stop(1.0, &color_base);
        } else {
            let _ = grad.add_color_stop(0.0, &color_base);
            let _ = grad.add_color_stop(1.0, &color_tip);
        }

        self.ctx.set_shadow_blur(10.0);
        self.ctx.set_shadow_color(&color_tip);
        self.ctx.set_fill_style_canvas_gradient(&grad);
        self.ctx.fill_rect(x, y, w, h);
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_spectrum_peak(&self, x: f64, y: f64, w: f64, hue: f64) {
        let color = format!("hsl({:.0}, 100%, 82%)", hue);
        self.ctx.set_fill_style_str(&color);
        self.ctx.set_shadow_blur(8.0);
        self.ctx.set_shadow_color(&color);
        self.ctx.fill_rect(x, y, w, 2.0);
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_wave(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);

        let point_count = self.wave_points.len();
        let half = height / 2.0;
        let amp = half * 0.82;

        // Subtle centre divider
        self.ctx.set_stroke_style_str("rgba(255,255,255,0.06)");
        self.ctx.set_line_width(1.0);
        self.ctx.begin_path();
        self.ctx.move_to(0.0, half);
        self.ctx.line_to(width, half);
        self.ctx.stroke();

        // Helper: draw one waveform channel
        let draw_channel = |points: &[f64], center_y: f64, color: &str, glow: &str| {
            // Wide glow pass
            self.ctx.set_line_width(7.0);
            self.ctx.set_stroke_style_str(glow);
            self.ctx.set_shadow_blur(0.0);
            self.ctx.begin_path();
            for i in 0..point_count {
                let x = (i as f64 / point_count as f64) * width;
                let y = center_y + points[i] * amp * 0.48;
                if i == 0 { self.ctx.move_to(x, y); } else { self.ctx.line_to(x, y); }
            }
            self.ctx.stroke();

            // Sharp main line with neon shadow
            self.ctx.set_line_width(2.0);
            self.ctx.set_stroke_style_str(color);
            self.ctx.set_shadow_blur(14.0);
            self.ctx.set_shadow_color(color);
            self.ctx.begin_path();
            for i in 0..point_count {
                let x = (i as f64 / point_count as f64) * width;
                let y = center_y + points[i] * amp * 0.48;
                if i == 0 { self.ctx.move_to(x, y); } else { self.ctx.line_to(x, y); }
            }
            self.ctx.stroke();
            self.ctx.set_shadow_blur(0.0);
        };

        // Left channel — top half, cyan (3 cycles)
        draw_channel(&self.wave_points, half / 2.0, "#00ffcc", "rgba(0,255,200,0.12)");
        // Right channel — bottom half, magenta (5 cycles)
        draw_channel(&self.wave_points_right, half + half / 2.0, "#ff44cc", "rgba(255,50,180,0.12)");
    }

    fn draw_circular(&self, width: f64, height: f64) {
        let center_x = width / 2.0;
        let center_y = height / 2.0;
        let base_radius = width.min(height) * 0.25;
        let bar_count = self.circular_bars.len();

        self.ctx.clear_rect(0.0, 0.0, width, height);

        for i in 0..bar_count {
            let angle = (i as f64 / bar_count as f64) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
            let value = self.circular_bars[i];
            let bar_length = (value / 255.0) * (width.min(height) * 0.35);

            let percent = (value / 255.0).min(1.0);
            let hue = 270.0 - percent * 270.0;
            let color = format!("hsl({:.0},100%,60%)", hue);

            let x1 = center_x + angle.cos() * base_radius;
            let y1 = center_y + angle.sin() * base_radius;
            let x2 = center_x + angle.cos() * (base_radius + bar_length);
            let y2 = center_y + angle.sin() * (base_radius + bar_length);

            self.ctx.set_shadow_blur(10.0);
            self.ctx.set_shadow_color(&color);
            self.ctx.set_stroke_style_str(&color);
            self.ctx.set_line_width(4.0);
            self.ctx.set_line_cap("round");

            self.ctx.begin_path();
            self.ctx.move_to(x1, y1);
            self.ctx.line_to(x2, y2);
            self.ctx.stroke();
        }

        self.ctx.set_shadow_blur(0.0);

        self.ctx.set_stroke_style_str("rgba(255, 255, 255, 0.1)");
        self.ctx.set_line_width(2.0);
        self.ctx.begin_path();
        let _ = self
            .ctx
            .arc(center_x, center_y, base_radius, 0.0, std::f64::consts::TAU);
        self.ctx.stroke();
    }

    fn draw_neon_bar(&self, x: f64, y: f64, w: f64, h: f64, value: f64, peak: f64) {
        let percent  = (value / 255.0).min(1.0);
        let fill_w   = w * percent;
        let peak_pct = (peak / 255.0).min(1.0);
        let glow_hue = 220.0 - percent * 220.0; // 220° blue → 0° red

        // === Ghost track (full width, dim gradient shows potential max) ===
        let bg = self.ctx.create_linear_gradient(x, y, x + w, y);
        let _ = bg.add_color_stop(0.0,  "rgba(30, 80, 255, 0.13)");
        let _ = bg.add_color_stop(0.45, "rgba(0,  200, 80, 0.13)");
        let _ = bg.add_color_stop(0.75, "rgba(255, 210, 0, 0.13)");
        let _ = bg.add_color_stop(1.0,  "rgba(255, 20,  0, 0.13)");
        self.ctx.set_fill_style_canvas_gradient(&bg);
        self.ctx.fill_rect(x, y, w, h);

        // Track border
        self.ctx.set_stroke_style_str("rgba(255,255,255,0.07)");
        self.ctx.set_line_width(1.0);
        self.ctx.stroke_rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0);

        if fill_w > 0.5 {
            // === Rainbow fill bar ===
            let fg = self.ctx.create_linear_gradient(x, y, x + w, y);
            let _ = fg.add_color_stop(0.0,  "hsl(220, 100%, 58%)");
            let _ = fg.add_color_stop(0.35, "hsl(165, 100%, 52%)");
            let _ = fg.add_color_stop(0.6,  "hsl(100, 100%, 48%)");
            let _ = fg.add_color_stop(0.78, "hsl(55,  100%, 50%)");
            let _ = fg.add_color_stop(0.92, "hsl(25,  100%, 54%)");
            let _ = fg.add_color_stop(1.0,  "hsl(0,   100%, 54%)");
            self.ctx.set_shadow_blur(18.0);
            self.ctx.set_shadow_color(&format!("hsla({:.0},100%,60%,0.75)", glow_hue));
            self.ctx.set_fill_style_canvas_gradient(&fg);
            self.ctx.fill_rect(x, y, fill_w, h);
            self.ctx.set_shadow_blur(0.0);

            // Top highlight stripe — makes bar look lit from above
            let hi = self.ctx.create_linear_gradient(x, y, x + w, y);
            let _ = hi.add_color_stop(0.0,  "rgba(130, 170, 255, 0.50)");
            let _ = hi.add_color_stop(0.45, "rgba(130, 255, 200, 0.50)");
            let _ = hi.add_color_stop(1.0,  "rgba(255, 140, 100, 0.50)");
            self.ctx.set_fill_style_canvas_gradient(&hi);
            self.ctx.fill_rect(x, y, fill_w, (h * 0.20).max(2.0));

            // Bright leading edge flash
            let edge_color = format!("hsl({:.0},100%,82%)", glow_hue);
            self.ctx.set_shadow_blur(14.0);
            self.ctx.set_shadow_color(&edge_color);
            self.ctx.set_fill_style_str(&edge_color);
            let edge_x = (x + fill_w - 2.0).max(x);
            self.ctx.fill_rect(edge_x, y, fill_w.min(2.0), h);
            self.ctx.set_shadow_blur(0.0);
        }

        // === Peak hold marker ===
        if peak > 3.0 {
            let phue  = 220.0 - peak_pct * 220.0;
            let pcol  = format!("hsl({:.0},100%,85%)", phue);
            let peak_x = x + w * peak_pct;
            self.ctx.set_shadow_blur(12.0);
            self.ctx.set_shadow_color(&pcol);
            self.ctx.set_fill_style_str(&pcol);
            self.ctx.fill_rect((peak_x - 1.5).max(x), y, 3.0_f64.min(w), h);
            self.ctx.set_shadow_blur(0.0);
        }

        // === Tick marks at 25 / 50 / 75 / 90 % ===
        self.ctx.set_stroke_style_str("rgba(255,255,255,0.20)");
        self.ctx.set_line_width(1.0);
        for &t in &[0.25_f64, 0.5, 0.75, 0.9] {
            let tx = x + w * t;
            self.ctx.begin_path();
            self.ctx.move_to(tx, y + h * 0.5);
            self.ctx.line_to(tx, y + h);
            self.ctx.stroke();
        }
    }

    fn draw_lissajous(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);

        let cx = width / 2.0;
        let cy = height / 2.0;
        // Amplitudes from L/R channels; ensure a minimum so the figure is visible
        let ax = ((self.smoothed_left / 255.0) * cx * 0.88).max(cx * 0.1);
        let ay = ((self.smoothed_right / 255.0) * cy * 0.88).max(cy * 0.1);

        self.ctx.set_line_width(1.5);
        self.ctx.set_stroke_style_str("#00ffcc");
        self.ctx.set_shadow_blur(12.0);
        self.ctx.set_shadow_color("#00ffcc");

        // 3:2 Lissajous — classic "figure-8 variant"
        self.ctx.begin_path();
        let steps = 400;
        for i in 0..=steps {
            let t = (i as f64 / steps as f64) * std::f64::consts::TAU;
            let x = cx + ax * (3.0 * t + self.lissajous_phase).sin();
            let y = cy + ay * (2.0 * t).sin();
            if i == 0 {
                self.ctx.move_to(x, y);
            } else {
                self.ctx.line_to(x, y);
            }
        }
        self.ctx.stroke();
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_particles(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);

        for p in &self.particles {
            let life = p[4];
            let size = (1.5 + (1.0 - life) * 2.5).max(0.5);
            let color = format!("rgba(0, 255, 180, {})", life);
            self.ctx.set_fill_style_str(&color);
            self.ctx.set_shadow_blur(10.0);
            self.ctx.set_shadow_color(&color);
            self.ctx.begin_path();
            let _ = self.ctx.arc(p[0], p[1], size, 0.0, std::f64::consts::TAU);
            self.ctx.fill();
        }
        self.ctx.set_shadow_blur(0.0);
    }

    /// Renamed from Mirror: expanding ellipse rings driven by audio volume.
    fn draw_mirror(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);

        let cx = width / 2.0;
        let cy = height / 2.0;

        for ring in &self.pulse_rings {
            let radius_norm = ring[0];
            let life = ring[1];
            let hue = ring[2];

            // Scale so radius=1.0 just reaches the canvas edges
            let rx = (cx * radius_norm * 1.15).max(0.1);
            let ry = (cy * radius_norm * 1.15).max(0.1);

            let line_w = (life * 3.5 + 0.5).max(0.5);
            let color = format!("hsla({:.0},100%,65%,{:.3})", hue, life);
            let glow  = format!("hsl({:.0},100%,65%)", hue);

            self.ctx.set_stroke_style_str(&color);
            self.ctx.set_shadow_blur(life * 22.0);
            self.ctx.set_shadow_color(&glow);
            self.ctx.set_line_width(line_w);

            self.ctx.begin_path();
            let _ = self.ctx.ellipse(cx, cy, rx, ry, 0.0, 0.0, std::f64::consts::TAU);
            self.ctx.stroke();
        }
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_starfield(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);
        let cx = width / 2.0;
        let cy = height / 2.0;
        let scale = width.min(height) * 0.5;
        let avg = (self.smoothed_left + self.smoothed_right) / 2.0;
        let intensity = avg / 255.0;

        for star in &self.stars {
            let z = star[2];
            let prev_z = star[3];
            let sx     = (star[0] / z)      * scale + cx;
            let sy     = (star[1] / z)      * scale * (height / width) + cy;
            let prev_sx = (star[0] / prev_z) * scale + cx;
            let prev_sy = (star[1] / prev_z) * scale * (height / width) + cy;

            if sx < -10.0 || sx > width + 10.0 || sy < -10.0 || sy > height + 10.0 {
                continue;
            }

            let brightness = 1.0 - z;
            let line_w = (brightness * 2.5 + 0.3).max(0.3);
            let hue = 200.0 + intensity * 50.0;
            let color = format!("hsla({:.0},70%,90%,{:.2})", hue, brightness);

            self.ctx.set_stroke_style_str(&color);
            self.ctx.set_line_width(line_w);
            self.ctx.set_shadow_blur(line_w * 3.0);
            self.ctx.set_shadow_color(&color);
            self.ctx.begin_path();
            self.ctx.move_to(prev_sx, prev_sy);
            self.ctx.line_to(sx, sy);
            self.ctx.stroke();
        }
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_dna(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);
        let mid_y = height / 2.0;
        let amp_l = (self.smoothed_left  / 255.0).max(0.08);
        let amp_r = (self.smoothed_right / 255.0).max(0.08);
        let amp   = mid_y * 0.82;
        let steps = 80usize;

        // Rungs
        for i in 0..=steps {
            let t     = i as f64 / steps as f64;
            let x     = t * width;
            let phase = t * std::f64::consts::TAU * 2.5 + self.wave_phase;
            let y1    = mid_y + phase.sin()                           * amp * amp_l;
            let y2    = mid_y + (phase + std::f64::consts::PI).sin() * amp * amp_r;
            let persp = (phase.cos() + 1.0) * 0.5;
            if persp < 0.08 { continue; }
            let hue   = t * 180.0 + 180.0;
            let color = format!("hsla({:.0},100%,70%,{:.2})", hue, persp * 0.55);
            self.ctx.set_stroke_style_str(&color);
            self.ctx.set_line_width(persp * 2.5 + 0.5);
            self.ctx.set_shadow_blur(persp * 8.0);
            self.ctx.set_shadow_color(&color);
            self.ctx.begin_path();
            self.ctx.move_to(x, y1);
            self.ctx.line_to(x, y2);
            self.ctx.stroke();
        }
        self.ctx.set_shadow_blur(0.0);

        // Strand 1 — cyan
        self.ctx.set_line_width(2.5);
        self.ctx.set_stroke_style_str("#00ffcc");
        self.ctx.set_shadow_blur(12.0);
        self.ctx.set_shadow_color("#00ffcc");
        self.ctx.begin_path();
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = t * width;
            let y = mid_y + (t * std::f64::consts::TAU * 2.5 + self.wave_phase).sin() * amp * amp_l;
            if i == 0 { self.ctx.move_to(x, y); } else { self.ctx.line_to(x, y); }
        }
        self.ctx.stroke();

        // Strand 2 — magenta
        self.ctx.set_stroke_style_str("#ff44cc");
        self.ctx.set_shadow_color("#ff44cc");
        self.ctx.begin_path();
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = t * width;
            let y = mid_y + (t * std::f64::consts::TAU * 2.5 + self.wave_phase + std::f64::consts::PI).sin() * amp * amp_r;
            if i == 0 { self.ctx.move_to(x, y); } else { self.ctx.line_to(x, y); }
        }
        self.ctx.stroke();
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_plasma(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);
        let t         = self.plasma_time;
        let intensity = (self.smoothed_left + self.smoothed_right) / 510.0;
        let r         = (width.min(height) * (0.5 + intensity * 0.4)).max(10.0);

        let _ = self.ctx.set_global_composite_operation("screen");

        let blobs: [(f64, f64, f64); 3] = [
            (
                width  * (0.5 + 0.42 * (t * 0.71).sin()),
                height * (0.5 + 0.42 * (t * 0.89).cos()),
                190.0 + t.sin() * 40.0,
            ),
            (
                width  * (0.5 + 0.42 * (t * 1.13).cos()),
                height * (0.5 + 0.42 * (t * 0.67).sin()),
                270.0 + t.cos() * 40.0,
            ),
            (
                width  * (0.5 + 0.35 * (t * 0.83).sin()),
                height * (0.5 + 0.35 * (t * 1.17).cos()),
                330.0 + (t * 1.3).sin() * 40.0,
            ),
        ];

        for (cx, cy, hue) in blobs {
            if let Ok(grad) = self.ctx.create_radial_gradient(cx, cy, 0.0, cx, cy, r) {
                let _ = grad.add_color_stop(0.0, &format!("hsla({:.0},100%,65%,0.75)", hue));
                let _ = grad.add_color_stop(1.0, &format!("hsla({:.0},100%,65%,0.0)",  hue));
                self.ctx.set_fill_style_canvas_gradient(&grad);
                self.ctx.fill_rect(0.0, 0.0, width, height);
            }
        }

        let _ = self.ctx.set_global_composite_operation("source-over");
    }

    fn draw_tunnel(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);
        let cx = width  / 2.0;
        let cy = height / 2.0;

        for (idx, ring) in self.pulse_rings.iter().enumerate() {
            let size = ring[0];
            let life = ring[1];
            let hue  = ring[2];
            let rx   = cx * size * 1.1;
            let ry   = cy * size * 1.1;
            if rx < 0.5 || ry < 0.5 { continue; }

            let rotation = self.lissajous_phase * 0.3 + idx as f64 * 0.12;
            let color    = format!("hsla({:.0},100%,65%,{:.3})", hue, life);
            let glow     = format!("hsl({:.0},100%,65%)", hue);
            let line_w   = (life * 2.5 + 0.5).max(0.5);

            self.ctx.save();
            let _ = self.ctx.translate(cx, cy);
            let _ = self.ctx.rotate(rotation);
            let _ = self.ctx.translate(-cx, -cy);

            self.ctx.set_stroke_style_str(&color);
            self.ctx.set_shadow_blur(life * 18.0);
            self.ctx.set_shadow_color(&glow);
            self.ctx.set_line_width(line_w);
            self.ctx.stroke_rect(cx - rx, cy - ry, rx * 2.0, ry * 2.0);

            self.ctx.restore();
        }
        self.ctx.set_shadow_blur(0.0);
    }

    fn draw_bounce(&self, width: f64, height: f64) {
        self.ctx.clear_rect(0.0, 0.0, width, height);
        let n        = self.bounce_balls.len();
        let max_dist = width.min(height) * 0.6;

        // Connecting lines
        for i in 0..n {
            for j in (i + 1)..n {
                let x1 = self.bounce_balls[i][0] * width;
                let y1 = self.bounce_balls[i][1] * height;
                let x2 = self.bounce_balls[j][0] * width;
                let y2 = self.bounce_balls[j][1] * height;
                let dist = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                if dist < max_dist {
                    let alpha = (1.0 - dist / max_dist) * 0.45;
                    let hue   = (self.bounce_balls[i][4] + self.bounce_balls[j][4]) / 2.0;
                    self.ctx.set_stroke_style_str(&format!("hsla({:.0},100%,65%,{:.2})", hue, alpha));
                    self.ctx.set_line_width(1.5);
                    self.ctx.set_shadow_blur(0.0);
                    self.ctx.begin_path();
                    self.ctx.move_to(x1, y1);
                    self.ctx.line_to(x2, y2);
                    self.ctx.stroke();
                }
            }
        }

        // Balls
        for ball in &self.bounce_balls {
            let x     = ball[0] * width;
            let y     = ball[1] * height;
            let hue   = ball[4];
            let color = format!("hsl({:.0},100%,70%)", hue);
            self.ctx.set_fill_style_str(&color);
            self.ctx.set_shadow_blur(20.0);
            self.ctx.set_shadow_color(&color);
            self.ctx.begin_path();
            let _ = self.ctx.arc(x, y, 6.0, 0.0, std::f64::consts::TAU);
            self.ctx.fill();
        }
        self.ctx.set_shadow_blur(0.0);
    }
}
