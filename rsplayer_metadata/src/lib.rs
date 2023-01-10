use std::sync::{Arc, Mutex};

use metadata::MetadataService;

pub mod metadata;
pub mod playlist;
pub mod queue;
#[cfg(test)]
mod test;

pub type MutArcMetadataSvc = Arc<Mutex<MetadataService>>;
