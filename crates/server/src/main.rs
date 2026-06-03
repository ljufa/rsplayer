extern crate env_logger;
#[macro_use]
extern crate log;

#[cfg(feature = "console-subscriber")]
use std::time::Duration;

use env_logger::Env;

#[allow(clippy::redundant_pub_crate, clippy::too_many_lines)]
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    #[cfg(feature = "console-subscriber")]
    console_subscriber::ConsoleLayer::builder()
        .retention(Duration::from_mins(1))
        .server_addr(([0, 0, 0, 0], 6669))
        .init();
    let version = env!("APP_VERSION");
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
            /     /
            by https://github.com/ljufa/rsplayer

        -------------------------------------------------------------------------
    "
    );

    rsplayer::run_backend::<std::path::PathBuf>(None).await;
}
