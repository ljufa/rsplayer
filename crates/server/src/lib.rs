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
use tokio::sync::mpsc;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::{select, spawn};

use crate::composition_root::{build_app_container, AppContainer, BuildOutcome};
use crate::mount_service::MountService;
use api_models::common::UserCommand;
use config::{ArcConfiguration, Configuration};

use env_logger::Env;

pub async fn run_backend(shutdown_rx: Option<Receiver<()>>, command_sender_out: Option<Sender<mpsc::Sender<UserCommand>>>) {
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

    let config = Configuration::new(&shared_db);
    info!("Configuration successfully loaded.");

    MountService::mount_all(&config.get_settings().network_storage_settings);

    let container = match build_app_container(&config, &shared_db) {
        BuildOutcome::Ready(c) => c,
        BuildOutcome::Degraded(e) => {
            start_degraded(&e, &config).await;
            return;
        }
    };

    run(container, shutdown_rx, command_sender_out, &config, &shared_db).await;

    info!("RSPlayer shutdown completed.");
}

async fn run(
    container: Box<AppContainer>,
    shutdown_rx: Option<Receiver<()>>,
    command_sender_out: Option<Sender<mpsc::Sender<UserCommand>>>,
    config: &ArcConfiguration,
    shared_db: &Arc<fjall::Database>,
) {
    let AppContainer {
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

    if let Some(out) = command_sender_out {
        let _ = out.send(player_commands_tx.clone());
    }

    let (http_server_future, https_server_future, websocket_future) =
        server::start(state_changes_tx.subscribe(), player_commands_tx.clone(), config);
    info!("HTTP servers started.");

    if let Some(service) = usb_service.clone() {
        usb::spawn_receiver_thread(
            service.clone(),
            player_commands_tx.clone(),
            system_commands_tx.clone(),
            state_changes_tx.clone(),
        );
        usb::spawn_sender_thread(service, &state_changes_tx);
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
                    player_service,
                    metadata_service,
                    playlist_service,
                    queue_service,
                    album_repository,
                    song_repository,
                    loudness_repository,
                    config.clone(),
                    player_commands_rx,
                    system_commands_tx,
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

        () = terminate_signal() => {
            info!("Terminate signal received.");
            persist_db_on_shutdown(shared_db);
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
            persist_db_on_shutdown(shared_db);
        }

        () = shutdown_fut => {
            info!("Shutdown channel triggered.");
            persist_db_on_shutdown(shared_db);
        }
    };
}

fn persist_db_on_shutdown(db: &fjall::Database) {
    info!("Persisting database to WAL...");
    let _ = db.persist(PersistMode::SyncAll);
}

fn terminate_signal() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    #[cfg(unix)]
    {
        Box::pin(async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sig = signal(SignalKind::terminate()).expect("failed to create SIGTERM handler");
            sig.recv().await;
        })
    }
    #[cfg(not(unix))]
    {
        Box::pin(std::future::pending::<()>())
    }
}

#[allow(clippy::redundant_pub_crate)]
async fn start_degraded(error: &anyhow::Error, config: &Arc<Configuration>) {
    warn!("Starting server in degraded mode.");
    let http_server_future = server::start_degraded(config, error);
    select! {
        () = http_server_future => {}

        () = terminate_signal() => {
            info!("Terminate signal received.");
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-c signal received.");
        }
    }
}
