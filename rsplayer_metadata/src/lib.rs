use std::sync::{Arc, Mutex};

use metadata::MetadataService;

pub mod metadata;
pub mod playback_queue;

pub type MutArcMetadataSvc = Arc<Mutex<MetadataService>>;