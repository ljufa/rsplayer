//! Shared data model for the backend and the web UI.
//!
//! Everything here is serde-serializable: these types cross the HTTP/WebSocket
//! API as JSON and are persisted by `rsplayer_config` and `rsplayer_metadata`,
//! so changes must stay backward-compatible (add fields with `#[serde(default)]`;
//! do not rename serialized names — see `AlsaSettings` for a historical example).
//!
//! Layout: [`common`] — commands and small shared value types; [`state`] —
//! `StateChangeEvent`, the one enum broadcast to every WebSocket client;
//! [`settings`] — the persisted `Settings` tree edited in the UI; [`player`],
//! [`playlist`], [`stat`] — songs, albums/playlists and library statistics;
//! [`podcast`] — subscriptions, episodes and their commands.

pub mod common;
pub mod player;
pub mod playlist;
pub mod podcast;
pub mod settings;
pub mod stat;
pub mod state;
pub use num_derive;
pub use num_traits;
pub use serde;
pub use serde_json;
pub use validator;
