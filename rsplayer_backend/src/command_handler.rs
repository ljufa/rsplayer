use log::debug;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::Receiver;

use api_models::common::UserCommand::{self, Metadata, Player, Playlist, Queue, Storage, UpdateDsp};
use api_models::state::StateChangeEvent;

use crate::command_context::{CommandContext, SystemCommandContext};
use crate::metadata_commands::handle_metadata_command;
use crate::player_commands::handle_player_command;
use crate::playlist_commands::handle_playlist_command;
use crate::queue_commands::handle_queue_command;
use crate::storage_commands::handle_storage_command;
use crate::system_commands::handle_system_command;

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn handle_user_commands(
    player_service: std::sync::Arc<rsplayer_playback::rsp::player_service::PlayerService>,
    metadata_service: std::sync::Arc<rsplayer_metadata::metadata_service::MetadataService>,
    playlist_service: std::sync::Arc<rsplayer_metadata::playlist_service::PlaylistService>,
    queue_service: std::sync::Arc<rsplayer_metadata::queue_service::QueueService>,
    album_repository: std::sync::Arc<rsplayer_metadata::album_repository::AlbumRepository>,
    song_repository: std::sync::Arc<rsplayer_metadata::song_repository::SongRepository>,
    loudness_repository: std::sync::Arc<rsplayer_metadata::loudness_repository::LoudnessRepository>,
    config_store: rsplayer_config::ArcConfiguration,
    mut input_commands_rx: Receiver<UserCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    let ctx = CommandContext::new(
        player_service,
        metadata_service,
        playlist_service,
        queue_service,
        album_repository,
        song_repository,
        loudness_repository,
        config_store,
        state_changes_sender,
    );

    loop {
        let Some(cmd) = input_commands_rx.recv().await else {
            debug!("Wait in loop");
            continue;
        };
        debug!("Received command {cmd:?}");

        match cmd {
            Player(player_cmd) => {
                handle_player_command(player_cmd, &ctx);
            }
            Playlist(playlist_cmd) => {
                handle_playlist_command(playlist_cmd, &ctx);
            }
            Queue(queue_cmd) => {
                handle_queue_command(queue_cmd, &ctx);
            }
            Metadata(metadata_cmd) => {
                handle_metadata_command(metadata_cmd, &ctx);
            }
            Storage(storage_cmd) => {
                handle_storage_command(storage_cmd, &ctx);
            }
            UpdateDsp(dsp_settings) => {
                ctx.player_service.update_dsp_settings(&dsp_settings);
                let mut settings = ctx.config_store.get_settings();
                settings.rs_player_settings.dsp_settings = dsp_settings;
                ctx.config_store.save_settings(&settings);
                ctx.send_notification("DSP settings updated and saved");
            }
        }
    }
}

pub async fn handle_system_commands(
    ai_service: rsplayer_hardware::audio_device::audio_service::ArcAudioInterfaceSvc,
    usb_service: Option<rsplayer_hardware::usb::ArcUsbService>,
    config: rsplayer_config::ArcConfiguration,
    mut input_commands_rx: Receiver<api_models::common::SystemCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    let ctx = SystemCommandContext::new(ai_service, usb_service, config, state_changes_sender);

    loop {
        if let Some(cmd) = input_commands_rx.recv().await {
            debug!("Received command {cmd:?}");
            handle_system_command(cmd, &ctx).await;
        }
    }
}
