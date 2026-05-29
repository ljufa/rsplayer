pub mod demuxer;
pub mod scarletbook;

pub use demuxer::SacdIsoReader;
pub use scarletbook::{detect_sector_mode, read_areas, read_tracks, SacdArea, SacdTrack, SACD_SAMPLING_FREQUENCY};

pub const SACD_TRACK_MARKER: &str = "#SACD_";
