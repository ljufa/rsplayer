//! Headless server entry point; all logic lives in `rsplayer::run_backend`.

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    rsplayer::run_backend(None, None, None).await;
}
