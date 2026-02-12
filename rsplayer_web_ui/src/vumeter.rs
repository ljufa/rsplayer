use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
use wasm_bindgen::JsCast;

pub struct VUMeter {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    left: u8,
    right: u8,
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

        let grad = self.ctx.create_linear_gradient(x, 0.0, x + w, 0.0);
        // Errors in gradient add_color_stop are ignored for simplicity
        let _ = grad.add_color_stop(0.0, "#48c774");
        let _ = grad.add_color_stop(0.6, "#ffdd57");
        let _ = grad.add_color_stop(0.9, "#f14668");

        self.ctx.set_fill_style(&grad);
        self.ctx.fill_rect(x, y, fill_width, h);

        self.ctx.set_fill_style(&JsValue::from_str("rgba(255, 255, 255, 0.1)"));
        self.ctx.fill_rect(x + fill_width, y, w - fill_width, h);
    }
}
