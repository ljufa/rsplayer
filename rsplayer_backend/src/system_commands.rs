use std::process::exit;

use api_models::common::SystemCommand::{self, PowerOff, RestartRSPlayer, RestartSystem, SetVol, VolDown, VolUp};
use api_models::state::StateChangeEvent;
use log::info;

use crate::command_context::SystemCommandContext;

pub async fn handle_system_command(cmd: SystemCommand, ctx: &SystemCommandContext) {
    match cmd {
        SystemCommand::SetFirmwarePower(val) => {
            if let Some(service) = &ctx.usb_service {
                if let Err(e) = service.send_power_command(val) {
                    log::error!("Failed to send power command: {e}");
                }
            }
        }
        SetVol(val) => {
            let nv = ctx.audio_service.set_volume(val);
            let mut settings = ctx.config.get_settings();
            settings.volume_ctrl_settings.saved_volume = Some(val);
            ctx.config.save_settings(&settings);
            ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
        }
        VolUp => {
            let nv = ctx.audio_service.volume_up();
            if nv.current > 0 {
                ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
            }
        }
        VolDown => {
            let nv = ctx.audio_service.volume_down();
            if nv.current > 0 {
                ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
            }
        }
        PowerOff => {
            info!("Shutting down system");
            _ = std::process::Command::new("/usr/sbin/poweroff")
                .spawn()
                .expect("halt command failed")
                .wait();
        }
        RestartSystem => {
            info!("Restarting system");
            _ = std::process::Command::new("/usr/sbin/reboot")
                .spawn()
                .expect("halt command failed")
                .wait();
        }
        RestartRSPlayer => {
            info!("Restarting RSPlayer");
            exit(1);
        }
        SystemCommand::QueryCurrentVolume => {
            let vol = ctx.audio_service.get_volume();
            ctx.send_event(StateChangeEvent::VolumeChangeEvent(vol));
        }
    }
}
