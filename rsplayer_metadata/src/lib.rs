use std::sync::{Arc, Mutex};

use metadata::MetadataService;

pub mod metadata;

pub type MutArcMetadataSvc = Arc<Mutex<MetadataService>>;