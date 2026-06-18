#[allow(clippy::redundant_pub_crate, clippy::too_many_lines)]
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    rsplayer::run_backend(None).await;
}
