use core::result;
use std::time::Duration;

use api_models::player::{Command, StatusChangeEvent};
use failure::Error;
use tokio::sync::broadcast::Receiver;

pub type Result<T> = result::Result<T, Error>;

pub async fn no_op_future(){
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn logging_receiver_future(mut rx: Receiver<StatusChangeEvent>){
    loop {
        let r = rx.recv().await;
        info!("Event received: {:?}", r);
    }
}