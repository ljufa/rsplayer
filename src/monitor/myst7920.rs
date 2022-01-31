//! # ST7920
//!
//! This is a Rust driver library for LCD displays using the [ST7920] controller.
//!
//! It supports graphics mode of the controller, 128x64 in 1bpp. SPI connection to MCU is supported.
//!
//! The controller supports 1 bit-per-pixel displays, so an off-screen buffer has to be used to provide random access to pixels.
//!
//! Size of the buffer is 1024 bytes.
//!
//! The buffer has to be flushed to update the display after a group of draw calls has been completed.
//! The flush is not part of embedded-graphics API.

use num_derive::ToPrimitive;
use num_traits::ToPrimitive;

use embedded_hal::blocking::delay::DelayUs;
use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

#[derive(Debug)]
pub enum Error<CommError, PinError> {
    Comm(CommError),
    Pin(PinError),
}

/// ST7920 instructions.
#[derive(ToPrimitive)]
enum Instruction {
    BasicFunction = 0x30,
    ExtendedFunction = 0x34,
    ClearScreen = 0x01,
    EntryMode = 0x06,
    DisplayOnCursorOff = 0x0C,
    GraphicsOn = 0x36,
    SetGraphicsAddress = 0x80,
}

pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 64;
const ROW_SIZE: usize = (WIDTH / 8) as usize;
const BUFFER_SIZE: usize = ROW_SIZE * HEIGHT as usize;
const X_ADDR_DIV: u8 = 16;

pub struct ST7920<SPI, RST, CS>
where
    SPI: spi::Write<u8>,
    RST: OutputPin,
    CS: OutputPin,
{
    /// SPI pin
    spi: SPI,

    /// Reset pin.
    rst: RST,

    /// CS pin
    cs: Option<CS>,

    buffer: [u8; BUFFER_SIZE],

    flip: bool,
}

impl<SPI, RST, CS, PinError, SPIError> ST7920<SPI, RST, CS>
where
    SPI: spi::Write<u8, Error = SPIError>,
    RST: OutputPin<Error = PinError>,
    CS: OutputPin<Error = PinError>,
{
    /// Create a new [`ST7920<SPI, RST, CS>`] driver instance that uses SPI connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use st7920::ST7920;
    ///
    /// let result = ST7920::new(spi, GPIO::new(pins.p01), None, false);
    /// assert_eq!(result, );
    /// ```
    pub fn new(spi: SPI, rst: RST, cs: Option<CS>, flip: bool) -> Self {
        let buffer = [0; BUFFER_SIZE];

        ST7920 {
            spi,
            rst,
            cs,
            buffer,
            flip,
        }
    }

    fn enable_cs(&mut self, delay: &mut dyn DelayUs<u32>) -> Result<(), Error<SPIError, PinError>> {
        if let Some(cs) = self.cs.as_mut() {
            cs.set_high().map_err(Error::Pin)?;
            delay.delay_us(1);
        }
        Ok(())
    }

    fn disable_cs(
        &mut self,
        delay: &mut dyn DelayUs<u32>,
    ) -> Result<(), Error<SPIError, PinError>> {
        if let Some(cs) = self.cs.as_mut() {
            delay.delay_us(1);
            cs.set_high().map_err(Error::Pin)?;
        }
        Ok(())
    }

    /// Initialize the display controller
    pub fn init(&mut self, delay: &mut dyn DelayUs<u32>) -> Result<(), Error<SPIError, PinError>> {
        self.enable_cs(delay)?;
        self.hard_reset(delay)?;
        self.write_command(Instruction::BasicFunction)?;
        delay.delay_us(200);
        self.write_command(Instruction::DisplayOnCursorOff)?;
        delay.delay_us(100);
        self.write_command(Instruction::ClearScreen)?;
        delay.delay_us(10 * 1000);
        self.write_command(Instruction::EntryMode)?;
        delay.delay_us(100);
        self.write_command(Instruction::ExtendedFunction)?;
        delay.delay_us(10 * 1000);
        self.write_command(Instruction::GraphicsOn)?;
        delay.delay_us(100 * 1000);

        self.disable_cs(delay)?;
        Ok(())
    }

    fn hard_reset(
        &mut self,
        delay: &mut dyn DelayUs<u32>,
    ) -> Result<(), Error<SPIError, PinError>> {
        self.rst.set_low().map_err(Error::Pin)?;
        delay.delay_us(40 * 1000);
        self.rst.set_high().map_err(Error::Pin)?;
        delay.delay_us(40 * 1000);
        Ok(())
    }

    fn write_command(&mut self, command: Instruction) -> Result<(), Error<SPIError, PinError>> {
        self.write_command_param(command, 0)
    }

    fn write_command_param(
        &mut self,
        command: Instruction,
        param: u8,
    ) -> Result<(), Error<SPIError, PinError>> {
        let command_param = command.to_u8().unwrap() | param;
        let cmd: u8 = 0xF8;

        self.spi
            .write(&[cmd, command_param & 0xF0, (command_param << 4) & 0xF0])
            .map_err(Error::Comm)?;

        Ok(())
    }

    fn write_data(&mut self, data: u8) -> Result<(), Error<SPIError, PinError>> {
        self.spi
            .write(&[0xFA, data & 0xF0, (data << 4) & 0xF0])
            .map_err(Error::Comm)?;
        Ok(())
    }

    fn set_address(&mut self, x: u8, y: u8) -> Result<(), Error<SPIError, PinError>> {
        const HALF_HEIGHT: u8 = HEIGHT as u8 / 2;

        self.write_command_param(
            Instruction::SetGraphicsAddress,
            if y < HALF_HEIGHT { y } else { y - HALF_HEIGHT },
        )?;
        self.write_command_param(
            Instruction::SetGraphicsAddress,
            if y < HALF_HEIGHT {
                x / X_ADDR_DIV
            } else {
                x / X_ADDR_DIV + (WIDTH as u8 / X_ADDR_DIV)
            },
        )?;

        Ok(())
    }

    /// Modify the raw buffer. 1 byte (u8) = 8 pixels
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let mut st7920 = st7920::ST7920(...);
    /// // add crazy pattern
    /// st7920.modify_buffer(|x, y, v| {
    ///     if x % 2 == y % 2 {
    ///         v | 0b10101010
    ///     } else {
    ///         v
    ///     }
    /// });
    /// st7920.flush();
    /// ```
    pub fn modify_buffer(&mut self, f: fn(x: u8, y: u8, v: u8) -> u8) {
        for i in 0..BUFFER_SIZE {
            let row = i / ROW_SIZE;
            let column = i - (row * ROW_SIZE);
            self.buffer[i] = f(column as u8, row as u8, self.buffer[i]);
        }
    }

    /// clears the buffer but don't update the display
    pub fn clear_buffer(&mut self) {
        for i in 0..BUFFER_SIZE {
            self.buffer[i] = 0;
        }
    }

    /// Clear whole display area and clears the buffer
    pub fn clear(&mut self, delay: &mut dyn DelayUs<u32>) -> Result<(), Error<SPIError, PinError>> {
        self.clear_buffer();
        self.flush(delay)?;
        Ok(())
    }

    /// Flush buffer to update region of the display
    pub fn clear_buffer_region(
        &mut self,
        x: u8,
        mut y: u8,
        w: u8,
        h: u8,
        delay: &mut dyn DelayUs<u32>,
    ) -> Result<(), Error<SPIError, PinError>> {
        self.enable_cs(delay)?;

        let mut adj_x = x;
        if self.flip {
            y = HEIGHT as u8 - (y + h);
            adj_x = WIDTH as u8 - (x + w);
        }

        let start = adj_x / 8;
        let mut right = adj_x + w;
        let end = (right / 8) + 1;

        let start_gap = adj_x % 8;

        right = end * 8;

        let end_gap = 8 - (right % 8);

        let mut row_start = y as usize * ROW_SIZE;
        for _ in y..y + h {
            for x in start..end {
                let mut mask = 0xFF_u8;
                if x == start {
                    mask = 0xFF_u8 >> start_gap;
                }
                if x == end {
                    mask &= 0xFF_u8 >> end_gap;
                }

                let pos = row_start + x as usize;
                self.buffer[pos] &= !mask;
            }

            row_start += ROW_SIZE;
        }

        self.disable_cs(delay)?;
        Ok(())
    }

    /// Draw pixel
    ///
    /// Supported values are 0 and (not 0)
    pub fn set_pixel(&mut self, mut x: u8, mut y: u8, val: u8) {
        if self.flip {
            y = (HEIGHT - 1) as u8 - y;
            x = (WIDTH - 1) as u8 - x;
        }

        let x_mask = 0x80 >> (x % 8);
        if val != 0 {
            self.buffer[y as usize * ROW_SIZE + x as usize / 8] |= x_mask;
        } else {
            self.buffer[y as usize * ROW_SIZE + x as usize / 8] &= !x_mask;
        }
    }

    /// Flush buffer to update entire display
    pub fn flush(&mut self, delay: &mut dyn DelayUs<u32>) -> Result<(), Error<SPIError, PinError>> {
        self.enable_cs(delay)?;

        for y in 0..HEIGHT as u8 / 2 {
            self.set_address(0, y)?;

            let mut row_start = y as usize * ROW_SIZE;
            for x in 0..ROW_SIZE {
                self.write_data(self.buffer[row_start + x])?;
            }
            row_start += (HEIGHT as usize / 2) * ROW_SIZE;
            for x in 0..ROW_SIZE {
                self.write_data(self.buffer[row_start + x])?;
            }
        }

        self.disable_cs(delay)?;
        Ok(())
    }

    /// Flush buffer to update region of the display
    pub fn flush_region(
        &mut self,
        x: u8,
        mut y: u8,
        w: u8,
        h: u8,
        delay: &mut dyn DelayUs<u32>,
    ) -> Result<(), Error<SPIError, PinError>> {
        self.enable_cs(delay)?;

        let mut adj_x = x;
        if self.flip {
            y = HEIGHT as u8 - (y + h);
            adj_x = WIDTH as u8 - (x + w);
        }

        let mut left = adj_x - adj_x % X_ADDR_DIV;
        let mut right = (adj_x + w) - 1;
        right -= right % X_ADDR_DIV;
        right += X_ADDR_DIV;

        if left > adj_x {
            left -= X_ADDR_DIV; //make sure rightmost pixels are covered
        }

        let mut row_start = y as usize * ROW_SIZE;
        self.set_address(adj_x, y)?;
        for y in y..(y + h) {
            self.set_address(adj_x, y)?;

            for x in left / 8..right / 8 {
                self.write_data(self.buffer[row_start + x as usize])?;
            }

            row_start += ROW_SIZE;
        }

        self.disable_cs(delay)?;
        Ok(())
    }
}

// #[cfg(feature = "graphics")]
use embedded_graphics::{
    self, draw_target::DrawTarget, geometry::Point, pixelcolor::BinaryColor, prelude::*,
};

// #[cfg(feature = "graphics")]
impl<SPI, CS, RST, PinError, SPIError> OriginDimensions for ST7920<SPI, CS, RST>
where
    SPI: spi::Write<u8, Error = SPIError>,
    RST: OutputPin<Error = PinError>,
    CS: OutputPin<Error = PinError>,
{
    fn size(&self) -> Size {
        Size {
            width: WIDTH,
            height: HEIGHT,
        }
    }
}

// #[cfg(feature = "graphics")]
impl<SPI, CS, RST, PinError, SPIError> DrawTarget for ST7920<SPI, CS, RST>
where
    SPI: spi::Write<u8, Error = SPIError>,
    RST: OutputPin<Error = PinError>,
    CS: OutputPin<Error = PinError>,
{
    type Error = core::convert::Infallible;
    type Color = BinaryColor;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for p in pixels {
            let Pixel(coord, color) = p;
            let x = coord.x as u8;
            let y = coord.y as u8;
            let c = match color {
                BinaryColor::Off => 0,
                BinaryColor::On => 1,
            };
            self.set_pixel(x, y, c);
        }

        Ok(())
    }
}

// #[cfg(feature = "graphics")]
impl<SPI, RST, CS, PinError, SPIError> ST7920<SPI, RST, CS>
where
    SPI: spi::Write<u8, Error = SPIError>,
    RST: OutputPin<Error = PinError>,
    CS: OutputPin<Error = PinError>,
{
    pub fn flush_region_graphics(
        &mut self,
        region: (Point, Size),
        delay: &mut dyn DelayUs<u32>,
    ) -> Result<(), Error<SPIError, PinError>> {
        self.flush_region(
            region.0.x as u8,
            region.0.y as u8,
            region.1.width as u8,
            region.1.height as u8,
            delay,
        )
    }
}
