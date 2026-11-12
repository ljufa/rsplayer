//! Scrobbling to `ListenBrainz` and Last.fm: settings, commands and status.
//!
//! Both services are opt-in and off by default. The server reports "now
//! playing" and, once enough of a track was heard, a scrobble (see the
//! `scrobble` crate for the rules). [`ScrobbleSettings`] is part of the
//! persisted `Settings`; its secrets leave the server masked
//! ([`ScrobbleSettings::mask_secrets`]) and the Last.fm session is managed by
//! the server only (the connect flow in [`ScrobbleCommand`]).

use serde::{Deserialize, Serialize};

use crate::settings::SECRET_MASK;

/// The public `ListenBrainz` API. Self-hosted or compatible servers (Maloja,
/// a `ListenBrainz` instance) use their own base URL instead.
pub const DEFAULT_LISTENBRAINZ_URL: &str = "https://api.listenbrainz.org";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrobbleSettings {
    #[serde(default)]
    pub listenbrainz_enabled: bool,
    /// User token from <https://listenbrainz.org/settings/>.
    #[serde(default)]
    pub listenbrainz_token: String,
    /// API base URL, without the `/1/...` path.
    #[serde(default = "default_listenbrainz_url")]
    pub listenbrainz_url: String,
    #[serde(default)]
    pub lastfm_enabled: bool,
    /// The user's own API account (last.fm/api/account/create): write calls
    /// must be signed with its secret.
    #[serde(default)]
    pub lastfm_api_key: String,
    #[serde(default)]
    pub lastfm_api_secret: String,
    /// Set by the server when the user finished connecting; empty when not
    /// connected.
    #[serde(default)]
    pub lastfm_session_key: String,
    #[serde(default)]
    pub lastfm_username: String,
}

fn default_listenbrainz_url() -> String {
    DEFAULT_LISTENBRAINZ_URL.to_string()
}

impl Default for ScrobbleSettings {
    fn default() -> Self {
        Self {
            listenbrainz_enabled: false,
            listenbrainz_token: String::new(),
            listenbrainz_url: default_listenbrainz_url(),
            lastfm_enabled: false,
            lastfm_api_key: String::new(),
            lastfm_api_secret: String::new(),
            lastfm_session_key: String::new(),
            lastfm_username: String::new(),
        }
    }
}

impl ScrobbleSettings {
    /// Replaces stored secrets with [`SECRET_MASK`] before settings leave the server.
    pub fn mask_secrets(&mut self) {
        for secret in [&mut self.listenbrainz_token, &mut self.lastfm_api_secret, &mut self.lastfm_session_key] {
            if !secret.is_empty() {
                *secret = SECRET_MASK.to_string();
            }
        }
    }

    /// Restores what a client must not change from `stored`: secrets it sent
    /// back masked, and the Last.fm session, which only the connect and
    /// disconnect commands change (a client's copy may predate them).
    pub fn restore_secrets(&mut self, stored: &Self) {
        if self.listenbrainz_token == SECRET_MASK {
            self.listenbrainz_token.clone_from(&stored.listenbrainz_token);
        }
        if self.lastfm_api_secret == SECRET_MASK {
            self.lastfm_api_secret.clone_from(&stored.lastfm_api_secret);
        }
        self.lastfm_session_key.clone_from(&stored.lastfm_session_key);
        self.lastfm_username.clone_from(&stored.lastfm_username);
    }

    /// Whether `value` is the mask a client got instead of a stored secret.
    #[must_use]
    pub fn is_masked(value: &str) -> bool {
        value == SECRET_MASK
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrobbleCommand {
    /// Answered with a `ScrobbleStatusEvent`.
    QueryStatus,
    /// Checks a token against a server (the values typed in the form; a
    /// masked token means the stored one).
    TestListenBrainz { url: String, token: String },
    /// Starts the Last.fm desktop auth flow with these credentials (a masked
    /// secret means the stored one). The status event then carries the URL
    /// the user opens to approve access.
    LastFmStartAuth { api_key: String, api_secret: String },
    /// After the user approved access: fetches and stores the session.
    LastFmCompleteAuth,
    /// Forgets the Last.fm session.
    LastFmDisconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScrobbleStatus {
    /// Last.fm user the stored session belongs to; empty when not connected.
    pub lastfm_username: String,
    /// Page where the user approves access, while a connect is in progress.
    pub lastfm_auth_url: Option<String>,
    /// `ListenBrainz` user of the last successful token test.
    pub listenbrainz_username: Option<String>,
    /// Scrobbles waiting to be sent again (service unreachable).
    pub listenbrainz_queued: usize,
    pub lastfm_queued: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_restores_secrets() {
        let stored = ScrobbleSettings {
            listenbrainz_token: "lb-token".into(),
            lastfm_api_key: "key".into(),
            lastfm_api_secret: "secret".into(),
            lastfm_session_key: "session".into(),
            lastfm_username: "me".into(),
            ..Default::default()
        };
        let mut sent = stored.clone();
        sent.mask_secrets();
        assert_eq!(sent.listenbrainz_token, SECRET_MASK);
        assert_eq!(sent.lastfm_api_secret, SECRET_MASK);
        assert_eq!(sent.lastfm_session_key, SECRET_MASK);
        assert_eq!(sent.lastfm_api_key, "key");

        // A stale client copy: no session, masked secrets.
        let mut back = sent.clone();
        back.lastfm_session_key = String::new();
        back.lastfm_username = String::new();
        back.restore_secrets(&stored);
        assert_eq!(back, stored);

        // A new token typed by the user is kept.
        let mut changed = sent;
        changed.listenbrainz_token = "new".into();
        changed.restore_secrets(&stored);
        assert_eq!(changed.listenbrainz_token, "new");
    }

    #[test]
    fn old_settings_without_scrobble_section_deserialize() {
        let s: ScrobbleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ScrobbleSettings::default());
        assert_eq!(s.listenbrainz_url, DEFAULT_LISTENBRAINZ_URL);
        assert!(!s.listenbrainz_enabled && !s.lastfm_enabled);
    }
}
