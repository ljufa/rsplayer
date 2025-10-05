use rpi_embedded::uart::Uart;
use std::io;

pub struct UartClient {
    uart: Uart,
}

impl UartClient {
    pub fn new(path: &str, baud_rate: u32) -> io::Result<Self> {
        let uart = Uart::with_path(path, baud_rate, rpi_embedded::uart::Parity::None, 8, 1).map_err(io::Error::other)?;
        Ok(Self { uart })
    }

    pub fn send_command(&mut self, command: &str) -> io::Result<()> {
        let message = format!("{:<16}", command);
        self.uart.write(message).map_err(io::Error::other)?;
        Ok(())
    }

}