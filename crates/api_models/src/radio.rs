//! User-defined radio stations.
//!
//! A [`RadioStation`] is a stream the listener entered by hand (name + URL),
//! stored by the server so it survives queue clears and restarts. It is kept
//! separate from the radio-browser favourites, which are only remembered by
//! their `stationuuid` and resolved against the radio-browser API by the UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RadioStation {
    /// Server-assigned id. Empty when the UI sends a station to be created.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Stream URL, played by adding it to the queue like any other source.
    pub url: String,
    /// Optional station logo shown in the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default)]
    pub added_at: Option<DateTime<Utc>>,
}

impl RadioStation {
    /// Trims the text fields and checks that a station is usable: it needs a
    /// name and an `http(s)` stream URL. Returns the cleaned station.
    ///
    /// # Errors
    /// When the name is blank or the URL is blank or not `http(s)`.
    pub fn validated(&self) -> Result<Self, String> {
        let name = self.name.trim();
        let url = self.url.trim();
        if name.is_empty() {
            return Err("Station name must not be empty".to_string());
        }
        if url.is_empty() {
            return Err("Stream URL must not be empty".to_string());
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("Stream URL must start with http:// or https://".to_string());
        }
        let image_url = self
            .image_url
            .as_ref()
            .map(|i| i.trim().to_string())
            .filter(|i| !i.is_empty());
        Ok(Self {
            id: self.id.trim().to_string(),
            name: name.to_string(),
            url: url.to_string(),
            image_url,
            added_at: self.added_at,
        })
    }
}
