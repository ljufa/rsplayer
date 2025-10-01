use api_models::state::StateChangeEvent;
use log::debug;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

#[allow(dead_code)]
pub async fn no_op_future() {
    loop {
        tokio::time::sleep(Duration::from_secs(500)).await;
    }
}

#[allow(dead_code)]
pub async fn logging_receiver_future(mut rx: Receiver<StateChangeEvent>) {
    loop {
        let r = rx.recv().await;
        debug!("Event received: {:?}", r);
    }
}
