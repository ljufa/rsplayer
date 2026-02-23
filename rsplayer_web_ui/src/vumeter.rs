use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

pub struct VUMeter {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    left: u8,
    right: u8,
}

/// Read a CSS custom property from the <html> element at runtime.
fn css_var(name: &str) -> String {
    let result: Option<String> = (|| {
        let window = web_sys::window()?;
        let document = window.document()?;
        let el = document.document_element()?;
        let style = window.get_computed_style(&el).ok()??;
        let value = style.get_property_value(name).ok()?;
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })();
    result.unwrap_or_default()
}

impl VUMeter {
    pub fn new(canvas_id: &str) -> Option<Self> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let canvas = document.get_element_by_id(canvas_id)?;
        let canvas: HtmlCanvasElement = canvas.dyn_into().ok()?;

        let ctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;

        let mut meter = Self {
            canvas,
            ctx,
            left: 0,
            right: 0,
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
        // In Rust WASM, calling draw directly is synchronous.
        // requestAnimationFrame is a bit more complex to setup with closures,
        // but since we are driven by WebSocket events (approx 20Hz), direct draw is fine.
        self.draw();
    }

    fn draw(&self) {
        let width = self.canvas.width() as f64;
        let height = self.canvas.height() as f64;

        self.ctx.clear_rect(0.0, 0.0, width, height);

        let bar_height = height / 2.0 - 2.0;
        let middle = height / 2.0;

        self.draw_bar(0.0, 0.0, width, bar_height, self.left);
        self.draw_bar(0.0, middle + 2.0, width, bar_height, self.right);
    }

    fn draw_bar(&self, x: f64, y: f64, w: f64, h: f64, value: u8) {
        let max_val = 255.0;
        let percent = (value as f64 / max_val).min(1.0);
        let fill_width = w * percent;

        // Use theme CSS variables; fall back to neutral colours if unavailable.
        let accent = css_var("--accent");
        let color_low = if accent.is_empty() {
            "#48c774".to_string()
        } else {
            accent
        };
        let color_mid = css_var("--secondary-text");
        let color_mid = if color_mid.is_empty() {
            "#ffdd57".to_string()
        } else {
            color_mid
        };
        let border = css_var("--border-color");
        let color_high = if border.is_empty() {
            "#f14668".to_string()
        } else {
            border
        };

        let grad = self.ctx.create_linear_gradient(x, 0.0, x + w, 0.0);
        let _ = grad.add_color_stop(0.0, &color_low);
        let _ = grad.add_color_stop(0.7, &color_mid);
        let _ = grad.add_color_stop(1.0, &color_high);

        self.ctx.set_fill_style(&grad);
        self.ctx.fill_rect(x, y, fill_width, h);

        // Empty portion: use ui-elements colour at low opacity
        let ui = css_var("--ui-elements");
        let empty_color = if ui.is_empty() {
            "rgba(255, 255, 255, 0.1)".to_string()
        } else {
            // append alpha via rgba isn't possible with a hex var; use low opacity white overlay
            "rgba(255, 255, 255, 0.08)".to_string()
        };
        self.ctx.set_fill_style(&JsValue::from_str(&empty_color));
        self.ctx.fill_rect(x + fill_width, y, w - fill_width, h);
    }
}
