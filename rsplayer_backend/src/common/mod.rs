use core::result;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use api_models::state::StateChangeEvent;
use failure::Error;
use rsplayer_metadata::metadata::MetadataService;
use tokio::sync::broadcast::Receiver;

use crate::{
    audio_device::audio_service::AudioInterfaceService, config::Configuration,
    player::player_service::PlayerService,
};

pub type Result<T> = result::Result<T, Error>;
pub type MutArcConfiguration = Arc<Mutex<Configuration>>;
pub type MutArcPlayerService = Arc<Mutex<PlayerService>>;
pub type ArcAudioInterfaceSvc = Arc<AudioInterfaceService>;
pub type MutArcMetadataSvc = Arc<Mutex<MetadataService>>;

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
