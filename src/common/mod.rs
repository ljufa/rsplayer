use core::result;
use std::time::Duration;

use api_models::player::*;
use failure::Error;
use futures::Stream;
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;


pub const DPLAY_CONFIG_DIR_PATH: &str = ".dplay/";

pub type Result<T> = result::Result<T, Error>;

