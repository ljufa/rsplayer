use std::{io::Read, str::FromStr};

use api_models::{
    common::{PlayerCommand, SystemCommand, UserCommand},
    state::StateChangeEvent,
};
use log::{debug, error, info};
use rpi_embedded::uart::Uart;
use tokio::sync::{broadcast::Receiver, mpsc::Sender};

pub async fn send_state_events(
    mut state_changes_rx: Receiver<StateChangeEvent>,
    uart_settings: api_models::settings::UartCmdChannelSettings,
) {
    let mut uart = Uart::with_path(
        uart_settings.uart_path.clone(),
        uart_settings.baud_rate,
        rpi_embedded::uart::Parity::None,
        8,
        1,
    )
    .unwrap();
    loop {
        if let Ok(StateChangeEvent::StreamerStateEvent(streamer_state)) = state_changes_rx.recv().await {
            let current_volume = streamer_state.volume_state.current;
            let message = format!("{:<16}", format!("SetVol({})", current_volume));
            uart.write(message).unwrap();
        }
    }
}

pub async fn receive_commands(
    player_commands_tx: Sender<UserCommand>,
    system_commands_tx: Sender<SystemCommand>,
    uart_settings: api_models::settings::UartCmdChannelSettings,
) {
    use std::fs::OpenOptions;
    use tokio::io::unix::AsyncFd;

    // Open UART device
    let Ok(uart) = OpenOptions::new()
        .read(true)
        .write(true)
        .open(uart_settings.uart_path.clone())
    else {
        error!("Failed to open UART device at {}", uart_settings.uart_path);
        return;
    };

    // Wrap it in AsyncFd to make it non-blocking
    let async_uart = AsyncFd::new(uart).unwrap();

    let mut buffer = [0u8; 16];

    loop {
        let mut guard = async_uart.readable().await.unwrap();
        match guard.get_inner().read(&mut buffer) {
            Ok(_n) => {
                let msg = core::str::from_utf8(&buffer).unwrap().trim();
                info!("Received: {}", msg);
                match msg {
                    "PowerOff" => {
                        system_commands_tx.send(SystemCommand::PowerOff).await.expect("");
                    }
                    _ => {
                        if let Ok(pc) = PlayerCommand::from_str(msg) {
                            debug!("Parsed command: {:?}", pc);
                            player_commands_tx
                                .send(UserCommand::Player(pc))
                                .await
                                .expect("Unable to send command");
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // UART buffer is empty, continue
                info!("UART buffer is empty");
                continue;
            }
            Err(e) => {
                error!("Error reading from UART: {}", e);
                break;
            }
        }
        guard.clear_ready();
    }
}

pub fn get_all_serial_devices() -> Vec<String> {
    let mut devices = vec![];
    for entry in std::fs::read_dir("/dev").unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.iter().any(|s| s.to_str().unwrap().starts_with("ttyA")) {
            devices.push(path.to_str().unwrap().to_string());
        }
    }
    devices
}
