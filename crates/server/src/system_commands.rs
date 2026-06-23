use std::process::exit;

use api_models::common::SystemCommand::{self, PowerOff, RestartRSPlayer, RestartSystem, SetVol, ToggleMute, VolDown, VolUp};
use api_models::state::StateChangeEvent;
use log::info;

use crate::command_context::SystemCommandContext;

fn system_actions_allowed(ctx: &SystemCommandContext) -> bool {
    if std::env::var("DEMO_MODE").is_ok() || std::env::var("RSPLAYER_DESKTOP").is_ok() {
        ctx.send_event(StateChangeEvent::NotificationError("Not available in this mode".to_string()));
        return false;
    }
    true
}

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
        ToggleMute => {
            let current = ctx.audio_service.get_volume();
            let mut settings = ctx.config.get_settings();
            let nv = if current.current == 0 {
                let restore = settings.volume_ctrl_settings.volume_before_mute.unwrap_or(current.max / 2);
                settings.volume_ctrl_settings.volume_before_mute = None;
                settings.volume_ctrl_settings.saved_volume = Some(restore);
                ctx.audio_service.set_volume(restore)
            } else {
                settings.volume_ctrl_settings.volume_before_mute = Some(current.current);
                ctx.audio_service.set_volume(0)
            };
            ctx.config.save_settings(&settings);
            ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
        }
        VolUp => {
            let nv = ctx.audio_service.volume_up();
            if nv.current > 0 {
                let mut settings = ctx.config.get_settings();
                settings.volume_ctrl_settings.saved_volume = Some(nv.current);
                ctx.config.save_settings(&settings);
                ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
            }
        }
        VolDown => {
            let nv = ctx.audio_service.volume_down();
            if nv.current > 0 {
                let mut settings = ctx.config.get_settings();
                settings.volume_ctrl_settings.saved_volume = Some(nv.current);
                ctx.config.save_settings(&settings);
                ctx.send_event(StateChangeEvent::VolumeChangeEvent(nv));
            }
        }
        SystemCommand::ReportVolume(val) => {
            let mut settings = ctx.config.get_settings();
            settings.volume_ctrl_settings.saved_volume = Some(val);
            ctx.config.save_settings(&settings);
            ctx.send_event(StateChangeEvent::VolumeChangeEvent(api_models::common::Volume {
                current: val,
                ..Default::default()
            }));
        }

        PowerOff => {
            if !system_actions_allowed(ctx) {
                return;
            }
            info!("Shutting down system");
            #[cfg(target_os = "linux")]
            {
                _ = std::process::Command::new("/usr/sbin/poweroff").spawn();
            }
            #[cfg(not(target_os = "linux"))]
            {
                ctx.send_event(StateChangeEvent::NotificationError(
                    "Power control not supported on this platform".to_string(),
                ));
            }
        }
        RestartSystem => {
            if !system_actions_allowed(ctx) {
                return;
            }
            info!("Restarting system");
            #[cfg(target_os = "linux")]
            {
                _ = std::process::Command::new("/usr/sbin/reboot").spawn();
            }
            #[cfg(not(target_os = "linux"))]
            {
                ctx.send_event(StateChangeEvent::NotificationError(
                    "Power control not supported on this platform".to_string(),
                ));
            }
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
