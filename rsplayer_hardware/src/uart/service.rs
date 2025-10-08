use std::sync::{Arc, Mutex};

use crate::uart::client::UartClient;
use anyhow::Result;
use serialport::SerialPort;

pub struct UartService {
    client: Mutex<UartClient>,
}

impl UartService {
    pub fn new(path: &str, baud_rate: u32) -> Self {
        let client = UartClient::new(path, baud_rate).unwrap();
        Self { client: Mutex::new(client) }
    }

    pub fn send_command(&self, command: &str) {
        self.client.lock().unwrap().send_command(command).unwrap();
    }

    pub fn try_clone_port(&self) -> Result<Box<dyn SerialPort>> {
        self.client.lock().unwrap().try_clone_port()
    }
}

pub type ArcUartService = Arc<UartService>;
