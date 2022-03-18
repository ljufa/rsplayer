use core::result;

use failure::Error;

pub type Result<T> = result::Result<T, Error>;
