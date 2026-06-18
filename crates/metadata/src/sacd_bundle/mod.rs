pub mod demuxer;
pub mod scarletbook;

pub use demuxer::SacdIsoReader;
pub use scarletbook::{SACD_SAMPLING_FREQUENCY, SacdArea, SacdTrack, detect_sector_mode, read_areas, read_tracks};

pub const SACD_TRACK_MARKER: &str = "#SACD_";
