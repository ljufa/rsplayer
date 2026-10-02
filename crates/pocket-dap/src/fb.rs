use std::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::prelude::*;

pub struct Buffer<C, const W: u32, const H: u32> {
    pixels: Vec<C>,
}

impl<C: PixelColor + Copy, const W: u32, const H: u32> Buffer<C, W, H> {
    pub fn new(fill: C) -> Self {
        Self {
            pixels: vec![fill; (W * H) as usize],
        }
    }

    pub fn scaled_argb<F: Fn(C) -> u32>(&self, scale: usize, map: F) -> Vec<u32> {
        let sw = W as usize * scale;
        let sh = H as usize * scale;
        let mut out = vec![0u32; sw * sh];
        for y in 0..H as usize {
            for x in 0..W as usize {
                let color = map(self.pixels[y * W as usize + x]);
                for dy in 0..scale {
                    for dx in 0..scale {
                        out[(y * scale + dy) * sw + (x * scale + dx)] = color;
                    }
                }
            }
        }
        out
    }
}

impl<C: PixelColor, const W: u32, const H: u32> OriginDimensions for Buffer<C, W, H> {
    fn size(&self) -> Size {
        Size::new(W, H)
    }
}

impl<C: PixelColor, const W: u32, const H: u32> DrawTarget for Buffer<C, W, H> {
    type Color = C;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x < 0 || coord.y < 0 {
                continue;
            }
            let x = coord.x as u32;
            let y = coord.y as u32;
            if x < W && y < H {
                self.pixels[(y * W + x) as usize] = color;
            }
        }
        Ok(())
    }
}
