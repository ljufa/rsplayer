//! SACD ISO support.
//!
//! [`scarletbook`] parses the disc structure (areas, track list, sector
//! mode), [`SacdIsoReader`] exposes one DSD track as a Symphonia format
//! reader. The scanner expands an ISO into one virtual library song per
//! track, keyed `path#SACD_<n>` ([`SACD_TRACK_MARKER`]).

pub mod demuxer;
pub mod scarletbook;

pub use demuxer::SacdIsoReader;
pub use scarletbook::{SACD_SAMPLING_FREQUENCY, SacdArea, SacdTrack, detect_sector_mode, read_areas, read_tracks};

pub const SACD_TRACK_MARKER: &str = "#SACD_";
