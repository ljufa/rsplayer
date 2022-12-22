
use std::{
    sync::{Arc},
    time::Duration,
};
use api_models::state::StateChangeEvent;

use tokio::sync::broadcast::Receiver;


#[allow(dead_code)]
pub async fn no_op_future() {
    loop {
        tokio::time::sleep(Duration::from_secs(50)).await;
    }
}

#[allow(dead_code)]
pub async fn logging_receiver_future(mut rx: Receiver<StateChangeEvent>) {
    loop {
        let r = rx.recv().await;
        trace!("Event received: {:?}", r);
    }
}
