//! Headless server entry point; all logic lives in `rsplayer::run_backend`.

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    rsplayer::run_backend(None, None, None).await;
    // `run_backend` returns once shutdown is done (database persisted) or a
    // core task died. Exit right here: dropping the runtime would wait for
    // `spawn_blocking` work that never ends, leaving the process hanging
    // until systemd's stop timeout kills it, and never restarted after a
    // task failure.
    std::process::exit(0);
}
