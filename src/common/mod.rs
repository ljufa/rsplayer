use core::result;



use failure::Error;






pub const DPLAY_CONFIG_DIR_PATH: &str = ".dplay/";

pub type Result<T> = result::Result<T, Error>;

