use api_models::common::Command;
use cfg_if::cfg_if;
use tokio::sync::mpsc::Sender;

// todo implement settings.is_enabled check
pub async fn listen(input_commands_tx: Sender<Command>) {
    cfg_if! {
        if #[cfg(feature="hw_volume_control")] {
            hw_volume::listen(input_commands_tx).await;
        } else if #[cfg(not(feature="hw_volume_control"))] {
            crate::common::no_op_future().await;
        }
    }
}

#[cfg(feature = "hw_volume_control")]
mod hw_volume {
    use api_models::common::Command;

    use tokio::sync::mpsc::Sender;

    use evdev::{Device, InputEventKind};

    pub async fn listen(input_commands_tx: Sender<Command>) {
        info!("Start Volume Control thread.");
        let device = Device::open("/dev/input/by-path/platform-rotary@f-event")
            .expect("Error opening device");
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
    }
}
