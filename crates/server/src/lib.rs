extern crate log;
pub mod command_context;
pub mod command_handler;
pub mod composition_root;
pub mod metadata_commands;
#[cfg_attr(target_os = "linux", path = "mount_service_linux.rs")]
#[cfg_attr(not(target_os = "linux"), path = "mount_service_stub.rs")]
pub mod mount_service;
pub mod player_commands;
pub mod playlist_commands;
pub mod queue_commands;
pub mod server;
pub mod storage_commands;
pub mod system_commands;

use fjall::PersistMode;
use hardware::usb;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::signal::unix::{Signal, SignalKind};
use tokio::{select, spawn};

use crate::composition_root::{build, AppContainer, BuildOutcome};
use crate::mount_service::MountService;
use config::Configuration;

use env_logger::Env;

pub async fn run_backend(shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>) {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let version = env!("CARGO_PKG_VERSION");
    info!("Starting RSPlayer {version}.");
    info!(
        r"
        -------------------------------------------------------------------------
            ██████╗ ███████╗██████╗ ██╗      █████╗ ██╗   ██╗███████╗██████╗
            ██╔══██╗██╔════╝██╔══██╗██║     ██╔══██╗╚██╗ ██╔╝██╔════╝██╔══██╗
            ██████╔╝███████╗██████╔╝██║     ███████║ ╚████╔╝ █████╗  ██████╔╝
            ██╔══██╗╚════██║██╔═══╝ ██║     ██╔══██║  ╚██╔╝  ██╔══╝  ██╔══██╗
            ██║  ██║███████║██║     ███████╗██║  ██║   ██║   ███████╗██║  ██║
            ╚═╝  ╚═╝╚══════╝╚═╝     ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝
            /  {version}   /
            by https://github.com/ljufa/rsplayer
        -------------------------------------------------------------------------
    "
    );

    let shared_db = Arc::new(
        fjall::Database::builder("rsplayer.db")
            .open()
            .expect("Failed to open fjall database"),
    );
    info!("Shared database opened.");

    let config = Arc::new(Configuration::new(&shared_db));
    info!("Configuration successfully loaded.");

    MountService::mount_all(&config.get_settings().network_storage_settings);

    let mut term_signal = tokio::signal::unix::signal(SignalKind::terminate()).expect("failed to create signal future");

    let container = match build(config.clone(), shared_db.clone()) {
        BuildOutcome::Ready(c) => c,
        BuildOutcome::Degraded(e) => {
            start_degraded(&mut term_signal, &e, &config).await;
            return;
        }
    };

    run(container, term_signal, shutdown_rx).await;

    info!("RSPlayer shutdown completed.");
}

#[allow(clippy::redundant_pub_crate)]
async fn run(container: Box<AppContainer>, mut term_signal: Signal, shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>) {
    let AppContainer {
        config,
        shared_db,
        album_repository,
        song_repository,
        loudness_repository,
        metadata_service,
        playlist_service,
        queue_service,
        player_service,
        audio_service,
        usb_service,
        state_changes_tx,
        user_commands,
        system_commands,
        ..
    } = *container;

    let (player_commands_tx, player_commands_rx) = user_commands.split();
    let (system_commands_tx, system_commands_rx) = system_commands.split();

    let (http_server_future, https_server_future, websocket_future) =
        server::start(state_changes_tx.subscribe(), player_commands_tx.clone(), &config);
    info!("HTTP servers started.");

    if let Some(service) = usb_service.clone() {
        usb::start_listening(
            service.clone(),
            player_commands_tx.clone(),
            system_commands_tx.clone(),
            state_changes_tx.clone(),
        );
        usb::start_state_sync(service, &state_changes_tx);
    }
    if config.get_settings().auto_resume_playback {
        player_service.play_from_current_queue_song();
    }
    #[cfg(feature = "lirc")]
    {
        let player_commands_tx_clone = player_commands_tx.clone();
        let system_commands_tx_clone = system_commands_tx.clone();
        tokio::spawn(async move {
            match hardware::ir_service::IrService::new(player_commands_tx_clone, system_commands_tx_clone).await {
                Ok(mut ir_service) => {
                    info!("LIRC service successfully created.");
                    ir_service.run().await;
                }
                Err(e) => {
                    error!("Failed to create LIRC service: {e}");
                }
            }
        });
    }

    let https_server_future = async {
        if let Some(fut) = https_server_future {
            fut.await;
        } else {
            std::future::pending::<()>().await;
        }
    };

    let shutdown_fut = async {
        if let Some(rx) = shutdown_rx {
            rx.await.ok();
        } else {
            std::future::pending::<()>().await;
        }
    };

    select! {
        _ = spawn(command_handler::handle_user_commands(
                    player_service.clone(),
                    metadata_service.clone(),
                    playlist_service.clone(),
                    queue_service.clone(),
                    album_repository.clone(),
                    song_repository.clone(),
                    loudness_repository.clone(),
                    config.clone(),
                    player_commands_rx,
                    system_commands_tx.clone(),
                    state_changes_tx.clone()))
            => {
                error!("Exit from command handler thread.");
            }

        _ = spawn(command_handler::handle_system_commands(
                audio_service,
                usb_service.clone(),
                config.clone(),
                system_commands_rx,
                state_changes_tx.clone()))
            => {
                error!("Exit from command handler thread.");
            }

        _ = spawn(http_server_future) => {
            error!("Exit from http_server thread.");
        }

        _ = spawn(https_server_future) => {
            error!("Exit from https_server thread.");
        }

        _ = spawn(websocket_future) => {
            error!("Exit from websocket thread.");
        }

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
            persist_db_on_shutdown(&shared_db);
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
            persist_db_on_shutdown(&shared_db);
        }

        () = shutdown_fut => {
            info!("Shutdown channel triggered.");
            persist_db_on_shutdown(&shared_db);
        }
    };
}

fn persist_db_on_shutdown(db: &fjall::Database) {
    info!("Persisting database to WAL...");
    let _ = db.persist(PersistMode::SyncAll);
}

#[allow(clippy::redundant_pub_crate)]
async fn start_degraded(term_signal: &mut Signal, error: &anyhow::Error, config: &Arc<Configuration>) {
    warn!("Starting server in degraded mode.");
    let http_server_future = server::start_degraded(config, error);
    select! {
        () = http_server_future => {}

        _ = term_signal.recv() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    }
}
