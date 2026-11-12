//! Scrobbling: reports what `RSPlayer` plays to `ListenBrainz` and Last.fm.
//!
//! Opt-in per service, off by default. [`machine`] holds the rules (pure,
//! driven by `StateChangeEvent`s with the time passed in): "now playing" on
//! every start, one scrobble per play once half the track or 4 minutes were
//! heard, only for library songs (no radio, podcasts, or multiroom
//! followers). [`listenbrainz`] and [`lastfm`] are the blocking HTTP clients
//! (`ureq`), [`queue`] keeps scrobbles that could not be sent in the shared
//! fjall database, and [`service`] ties it together: an event task feeding
//! the state machine and a worker task doing all network calls off the
//! event path.

pub mod http;
pub mod lastfm;
pub mod listenbrainz;
pub mod machine;
pub mod queue;
pub mod service;
pub mod track;

pub use service::ScrobbleService;
