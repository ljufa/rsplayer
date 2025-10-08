use anyhow::{Ok, Result};
use log::info;
use rpi_embedded::uart::Uart;
use serialport::{SerialPort, StopBits};
use std::{io, time::Duration};

pub struct UartClient {
    // uart: Uart,
    port: Box<dyn SerialPort>,
}

impl UartClient {
    pub fn new(path: &str, baud_rate: u32) -> Result<Self> {
        let port = serialport::new(path, baud_rate)
            // .dtr_on_open(true)
            .timeout(Duration::from_secs(1))
            .open()
            .expect("Failed to open port");
        Ok(Self { port })
    }

    pub fn send_command(&mut self, command: &str) -> Result<()> {
        let message = format!("{}\n", command);
        self.port.write_all(message.as_bytes()).unwrap();
        info!("Written command: {}", command);
        self.port.flush().unwrap();
        Ok(())
    }

    pub fn try_clone_port(&self) -> Result<Box<dyn SerialPort>> {
        self.port.try_clone().map_err(anyhow::Error::from)
    }
}
