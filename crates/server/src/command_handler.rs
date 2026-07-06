//! Command dispatch.
//!
//! One task drains the `UserCommand` mpsc channel and routes each command
//! to its domain handler (`*_commands.rs`), all sharing [`CommandContext`].
//! A second task handles `SystemCommand`s (volume, power) against the
//! hardware audio service. Sequential by design — one command at a time,
//! state flows back via broadcast events.

use std::sync::Arc;

use config::ArcConfiguration;
use log::{debug, error};
use metadata::metadata_service::MetadataService;
use metadata::playlist_service::PlaylistService;
use metadata::ports::album_repository::ArcAlbumRepository;
use metadata::ports::loudness_repository::ArcLoudnessRepository;
use metadata::ports::song_repository::ArcSongRepository;
use metadata::queue_service::QueueService;
use playback::rsp::player_service::PlayerService;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::{self, Receiver};

use api_models::common::SystemCommand;
use api_models::common::UserCommand::{self, Metadata, Multiroom, Player, Playlist, Queue, Storage, System, UpdateDsp};
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
    player_service: Arc<PlayerService>,
    metadata_service: Arc<MetadataService>,
    playlist_service: Arc<PlaylistService>,
    queue_service: Arc<QueueService>,
    album_repository: ArcAlbumRepository,
    song_repository: ArcSongRepository,
    loudness_repository: ArcLoudnessRepository,
    config_store: ArcConfiguration,
    mut input_commands_rx: Receiver<UserCommand>,
    system_commands_tx: mpsc::Sender<SystemCommand>,
    multiroom_commands_tx: mpsc::Sender<api_models::common::MultiroomCommand>,
    multiroom_follower_active: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
        multiroom_follower_active,
        state_changes_sender,
    );

    // recv() returns None only when every sender is gone — the process is
    // shutting down, so exit rather than spin on a closed channel.
    while let Some(cmd) = input_commands_rx.recv().await {
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
            System(req) => {
                if let Err(e) = system_commands_tx.send(req.into()).await {
                    error!("Failed to forward SystemRequest to system handler: {e}");
                }
            }
            Multiroom(multiroom_cmd) => {
                // The receiver only exists while multiroom is enabled in settings.
                if let Err(e) = multiroom_commands_tx.send(multiroom_cmd).await {
                    debug!("Multiroom command dropped (multiroom disabled?): {e}");
                }
            }
        }
    }
}

pub async fn handle_system_commands(
    ai_service: hardware::audio_device::audio_service::ArcAudioInterfaceSvc,
    usb_service: Option<hardware::usb::ArcUsbService>,
    config: config::ArcConfiguration,
    mut input_commands_rx: Receiver<api_models::common::SystemCommand>,
    state_changes_sender: Sender<StateChangeEvent>,
) {
    let ctx = SystemCommandContext::new(ai_service, usb_service, config, state_changes_sender);

    while let Some(cmd) = input_commands_rx.recv().await {
        debug!("Received system command {cmd:?}");
        handle_system_command(cmd, &ctx).await;
    }
}
