use api_models::common::StorageCommand;
use api_models::state::StateChangeEvent;
use log::error;

use crate::command_context::CommandContext;

#[allow(clippy::too_many_lines)]
pub fn handle_storage_command(cmd: StorageCommand, ctx: &CommandContext) {
    match cmd {
        StorageCommand::Mount(mount_config) => match crate::mount_service::MountService::mount_share(&mount_config) {
            Ok(mount_point) => {
                let mut settings = ctx.config_store.get_settings();
                let existing = settings
                    .network_storage_settings
                    .mounts
                    .iter()
                    .position(|m| m.name == mount_config.name);
                if let Some(idx) = existing {
                    settings.network_storage_settings.mounts[idx] = mount_config;
                } else {
                    settings.network_storage_settings.mounts.push(mount_config);
                }
                if !settings.metadata_settings.music_directories.contains(&mount_point) {
                    settings.metadata_settings.music_directories.push(mount_point.clone());
                }
                ctx.config_store.save_settings(&settings);
                let statuses =
                    crate::mount_service::MountService::query_mount_status(&settings.network_storage_settings);
                ctx.send_event(StateChangeEvent::MountStatusEvent(statuses));
                ctx.send_notification(&format!("Mounted at {mount_point}"));
            }
            Err(e) => {
                error!("Mount failed: {e}");
                ctx.send_error(&format!("Mount failed: {e}"));
            }
        },
        StorageCommand::Unmount(name) => {
            let settings = ctx.config_store.get_settings();
            let config = settings.network_storage_settings.mounts.iter().find(|m| m.name == name);

            let ext_mount = config.and_then(|c| c.mount_point.as_deref());

            match crate::mount_service::MountService::unmount_share(&name, ext_mount) {
                Ok(()) => {
                    let statuses =
                        crate::mount_service::MountService::query_mount_status(&settings.network_storage_settings);
                    ctx.send_event(StateChangeEvent::MountStatusEvent(statuses));
                    ctx.send_notification(&format!("Unmounted {name}"));
                }
                Err(e) => {
                    error!("Unmount failed: {e}");
                    ctx.send_error(&format!("Unmount failed: {e}"));
                }
            }
        }
        StorageCommand::Remove(name) => {
            let mut settings = ctx.config_store.get_settings();
            let config = settings
                .network_storage_settings
                .mounts
                .iter()
                .find(|m| m.name == name)
                .cloned();
            let is_rsplayer_managed = config.as_ref().is_some_and(|c| c.mount_point.is_none());
            let mount_point = config
                .and_then(|c| c.mount_point)
                .unwrap_or_else(|| format!("/mnt/rsplayer/{name}"));

            if is_rsplayer_managed {
                let _ = crate::mount_service::MountService::unmount_share(&name, None);
            }

            settings.network_storage_settings.mounts.retain(|m| m.name != name);
            settings
                .metadata_settings
                .music_directories
                .retain(|d| d != &mount_point);
            ctx.config_store.save_settings(&settings);
            let statuses = crate::mount_service::MountService::query_mount_status(&settings.network_storage_settings);
            ctx.send_event(StateChangeEvent::MountStatusEvent(statuses));
            let external =
                crate::mount_service::MountService::discover_external_mounts(&settings.network_storage_settings);
            ctx.send_event(StateChangeEvent::ExternalMountsEvent(external));
            ctx.send_notification(&format!("Removed {name}"));
        }
        StorageCommand::QueryMountStatus => {
            let settings = ctx.config_store.get_settings();
            let statuses = crate::mount_service::MountService::query_mount_status(&settings.network_storage_settings);
            ctx.send_event(StateChangeEvent::MountStatusEvent(statuses));
            let dir_statuses = crate::mount_service::MountService::query_music_dir_status(&settings.metadata_settings);
            ctx.send_event(StateChangeEvent::MusicDirStatusEvent(dir_statuses));
            let external =
                crate::mount_service::MountService::discover_external_mounts(&settings.network_storage_settings);
            ctx.send_event(StateChangeEvent::ExternalMountsEvent(external));
        }
        StorageCommand::QueryMusicDirStatus => {
            let settings = ctx.config_store.get_settings();
            let dir_statuses = crate::mount_service::MountService::query_music_dir_status(&settings.metadata_settings);
            ctx.send_event(StateChangeEvent::MusicDirStatusEvent(dir_statuses));
        }
        StorageCommand::SaveExternalMount(mount_point) => {
            let mut settings = ctx.config_store.get_settings();
            let externals =
                crate::mount_service::MountService::discover_external_mounts(&settings.network_storage_settings);
            if let Some(ext) = externals.iter().find(|e| e.mount_point == mount_point) {
                if let Some(mut config) = crate::mount_service::MountService::parse_external_mount_to_config(ext) {
                    let base_name = config.name.clone();
                    let mut suffix = 0u32;
                    while settings
                        .network_storage_settings
                        .mounts
                        .iter()
                        .any(|m| m.name == config.name)
                    {
                        suffix += 1;
                        config.name = format!("{base_name}_{suffix}");
                    }

                    let mp = config.mount_point.clone().unwrap_or_default();
                    settings.network_storage_settings.mounts.push(config);
                    if !settings.metadata_settings.music_directories.contains(&mp) {
                        settings.metadata_settings.music_directories.push(mp);
                    }
                    ctx.config_store.save_settings(&settings);

                    let statuses =
                        crate::mount_service::MountService::query_mount_status(&settings.network_storage_settings);
                    ctx.send_event(StateChangeEvent::MountStatusEvent(statuses));
                    let external = crate::mount_service::MountService::discover_external_mounts(
                        &settings.network_storage_settings,
                    );
                    ctx.send_event(StateChangeEvent::ExternalMountsEvent(external));
                    ctx.send_notification(&format!("External mount saved: {mount_point}"));
                } else {
                    ctx.send_error("Failed to parse external mount");
                }
            } else {
                ctx.send_error(&format!("External mount not found: {mount_point}"));
            }
        }
    }
}
