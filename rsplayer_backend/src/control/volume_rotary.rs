use api_models::common::Command;
use tokio::sync::mpsc::Sender;

use crate::common::MutArcConfiguration;

// todo implement settings.is_enabled check
pub async fn listen(input_commands_tx: Sender<Command>, config: MutArcConfiguration) {
    let volume_settings = config
        .lock()
        .expect("Unable to lock config")
        .get_settings()
        .volume_ctrl_settings;
    if volume_settings.rotary_enabled {
        hw_volume::listen(input_commands_tx, volume_settings).await;
    } else {
        crate::common::no_op_future().await;
    }
}

mod hw_volume {
    use api_models::{common::Command, settings::VolumeControlSettings};

    use tokio::sync::mpsc::Sender;

    use evdev::{Device, InputEventKind};

    pub async fn listen(
        input_commands_tx: Sender<Command>,
        volume_settings: VolumeControlSettings,
    ) {
        if let Ok(device) = Device::open(volume_settings.rotary_event_device_path) {
            info!("Start Volume Control thread.");
            let mut events = device.into_event_stream().expect("Failed");
            loop {
                trace!("Loop cycle");
                let ev = events.next_event().await.expect("Error");
                match ev.kind() {
                    InputEventKind::RelAxis(_) => {
                        trace!("Event: {:?}", ev);
                        if ev.value() == 1 {
                            let _ = input_commands_tx.send(Command::VolDown).await;
                        } else {
                            let _ = input_commands_tx.send(Command::VolUp).await;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            error!("Error opening rotary volume control event device");
            crate::common::no_op_future().await;
        }
    }
}
