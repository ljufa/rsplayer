use tokio::sync::mpsc::Sender;

use api_models::common::SystemCommand;
use rsplayer_config::ArcConfiguration;

// todo implement settings.is_enabled check
pub async fn listen(system_commands_tx: Sender<SystemCommand>, config: ArcConfiguration) {
    let volume_settings = config.get_settings().volume_ctrl_settings;
    if volume_settings.rotary_enabled {
        hw_volume::listen(system_commands_tx, volume_settings).await;
    } else {
        crate::common::no_op_future().await;
    }
}

mod hw_volume {
    use evdev::{Device, InputEventKind};
    use log::{debug, error, info};
    use tokio::sync::mpsc::Sender;

    use api_models::{common::SystemCommand, settings::VolumeControlSettings};

    pub async fn listen(system_commands_tx: Sender<SystemCommand>, volume_settings: VolumeControlSettings) {
        if let Ok(device) = Device::open(volume_settings.rotary_event_device_path) {
            info!("Start Volume Control thread.");
            let mut events = device.into_event_stream().expect("Failed");
            loop {
                debug!("Loop cycle");
                let ev = events.next_event().await.expect("Error");
                if let InputEventKind::RelAxis(_) = ev.kind() {
                    debug!("Event: {:?}", ev);
                    if ev.value() == 1 {
                        system_commands_tx.send(SystemCommand::VolDown).await.expect("");
                    } else {
                        system_commands_tx.send(SystemCommand::VolUp).await.expect("");
                    }
                }
            }
        } else {
            error!("Error opening rotary volume control event device");
            crate::common::no_op_future().await;
        }
    }
}
