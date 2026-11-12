//! `ListenBrainz` client (<https://listenbrainz.readthedocs.io/en/latest/users/api/core.html>).
//!
//! Works with any server speaking the same API (a self-hosted `ListenBrainz`,
//! Maloja's `ListenBrainz` endpoint): the base URL comes from the settings.

use serde_json::{Map, Value, json};

use crate::http::{self, Reply, SubmitError, status_error_kind};
use crate::track::{Listen, ScrobbleTrack};

/// Listens per `import` request when the retry queue is flushed (the API
/// allows up to 1000).
pub const BATCH_SIZE: usize = 100;

const PLAYER_NAME: &str = "RSPlayer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenType {
    PlayingNow,
    Single,
    Import,
}

impl ListenType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PlayingNow => "playing_now",
            Self::Single => "single",
            Self::Import => "import",
        }
    }
}

pub struct ListenBrainzClient {
    base_url: String,
    token: String,
}

impl ListenBrainzClient {
    #[must_use]
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
        }
    }

    /// The user name the token belongs to.
    pub fn validate_token(&self) -> Result<String, SubmitError> {
        let reply = http::read(
            http::agent()
                .get(format!("{}/1/validate-token", self.base_url))
                .header("Authorization", format!("Token {}", self.token))
                .call(),
        )?;
        let body = reply.json().unwrap_or_default();
        if reply.status == 401 || body.get("valid").and_then(Value::as_bool) == Some(false) {
            return Err(SubmitError::Auth(message(&reply)));
        }
        check(&reply)?;
        body.get("user_name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SubmitError::Rejected("answer has no user name".into()))
    }

    pub fn playing_now(&self, track: &ScrobbleTrack) -> Result<(), SubmitError> {
        self.post(&payload(ListenType::PlayingNow, &[(track, None)]))
    }

    /// One listen as `single`, several (from the retry queue) as `import`.
    pub fn submit(&self, listens: &[Listen]) -> Result<(), SubmitError> {
        let kind = if listens.len() == 1 { ListenType::Single } else { ListenType::Import };
        let items: Vec<_> = listens.iter().map(|l| (&l.track, Some(l.listened_at))).collect();
        self.post(&payload(kind, &items))
    }

    fn post(&self, body: &Value) -> Result<(), SubmitError> {
        let reply = http::read(
            http::agent()
                .post(format!("{}/1/submit-listens", self.base_url))
                .header("Authorization", format!("Token {}", self.token))
                .header("Content-Type", "application/json")
                .send(body.to_string()),
        )?;
        check(&reply)
    }
}

fn check(reply: &Reply) -> Result<(), SubmitError> {
    status_error_kind(reply.status).map_or(Ok(()), |kind| Err(kind(message(reply))))
}

/// The server's error text, or the status.
fn message(reply: &Reply) -> String {
    let text = reply
        .json()
        .and_then(|v| v.get("error").or_else(|| v.get("message")).and_then(Value::as_str).map(str::to_owned));
    text.map_or_else(|| format!("HTTP {}", reply.status), |t| format!("HTTP {}: {t}", reply.status))
}

/// The `submit-listens` request body. `playing_now` items carry no time.
#[must_use]
pub fn payload(kind: ListenType, items: &[(&ScrobbleTrack, Option<i64>)]) -> Value {
    let listens: Vec<Value> = items
        .iter()
        .map(|(track, listened_at)| {
            let mut listen = Map::new();
            if kind != ListenType::PlayingNow
                && let Some(ts) = listened_at
            {
                listen.insert("listened_at".into(), json!(ts));
            }
            listen.insert("track_metadata".into(), track_metadata(track));
            Value::Object(listen)
        })
        .collect();
    json!({ "listen_type": kind.as_str(), "payload": listens })
}

fn track_metadata(track: &ScrobbleTrack) -> Value {
    let mut info = Map::new();
    info.insert("media_player".into(), json!(PLAYER_NAME));
    info.insert("media_player_version".into(), json!(env!("CARGO_PKG_VERSION")));
    info.insert("submission_client".into(), json!(PLAYER_NAME));
    info.insert("submission_client_version".into(), json!(env!("CARGO_PKG_VERSION")));
    if let Some(d) = track.duration_secs {
        info.insert("duration_ms".into(), json!(d * 1000));
    }
    if let Some(n) = track.track_number {
        info.insert("tracknumber".into(), json!(n));
    }
    if let Some(id) = &track.recording_mbid {
        info.insert("recording_mbid".into(), json!(id));
    }
    if let Some(id) = &track.release_mbid {
        info.insert("release_mbid".into(), json!(id));
    }
    if let Some(id) = &track.release_track_mbid {
        info.insert("track_mbid".into(), json!(id));
    }
    if !track.artist_mbids.is_empty() {
        info.insert("artist_mbids".into(), json!(track.artist_mbids));
    }
    if let Some(a) = &track.album_artist {
        info.insert("release_artist_name".into(), json!(a));
    }
    let mut meta = Map::new();
    meta.insert("artist_name".into(), json!(track.artist));
    meta.insert("track_name".into(), json!(track.title));
    if let Some(album) = &track.album {
        meta.insert("release_name".into(), json!(album));
    }
    meta.insert("additional_info".into(), Value::Object(info));
    Value::Object(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockServer;

    const MBID: &str = "0b1d6b7c-1234-4cde-8f00-aabbccddeeff";

    fn track() -> ScrobbleTrack {
        ScrobbleTrack {
            artist: "Artist".into(),
            title: "Title".into(),
            album: Some("Album".into()),
            duration_secs: Some(215),
            track_number: Some(4),
            recording_mbid: Some(MBID.into()),
            artist_mbids: vec![MBID.into()],
            ..Default::default()
        }
    }

    fn listen(ts: i64) -> Listen {
        Listen { track: track(), listened_at: ts }
    }

    #[test]
    fn single_payload_shape() {
        let t = track();
        let v = payload(ListenType::Single, &[(&t, Some(1_700_000_000))]);
        assert_eq!(v["listen_type"], "single");
        let item = &v["payload"][0];
        assert_eq!(item["listened_at"], 1_700_000_000);
        let meta = &item["track_metadata"];
        assert_eq!(meta["artist_name"], "Artist");
        assert_eq!(meta["track_name"], "Title");
        assert_eq!(meta["release_name"], "Album");
        let info = &meta["additional_info"];
        assert_eq!(info["duration_ms"], 215_000);
        assert_eq!(info["tracknumber"], 4);
        assert_eq!(info["media_player"], "RSPlayer");
        assert_eq!(info["submission_client"], "RSPlayer");
        assert_eq!(info["media_player_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(info["recording_mbid"], MBID);
        assert_eq!(info["artist_mbids"][0], MBID);
        assert!(info.get("release_mbid").is_none());
    }

    #[test]
    fn playing_now_has_no_timestamp() {
        let t = ScrobbleTrack {
            artist: "A".into(),
            title: "T".into(),
            ..Default::default()
        };
        let v = payload(ListenType::PlayingNow, &[(&t, Some(5))]);
        assert_eq!(v["listen_type"], "playing_now");
        assert!(v["payload"][0].get("listened_at").is_none());
        assert!(v["payload"][0]["track_metadata"].get("release_name").is_none());
    }

    #[test]
    fn submits_with_token_header() {
        let server = MockServer::start();
        let client = ListenBrainzClient::new(&format!("{}/", server.url), "tok");
        client.playing_now(&track()).unwrap();
        client.submit(&[listen(10)]).unwrap();
        client.submit(&[listen(10), listen(20)]).unwrap();
        let reqs = server.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs.iter().all(|r| r.method == "POST" && r.path == "/1/submit-listens"));
        assert!(reqs.iter().all(|r| r.header("authorization") == Some("Token tok")));
        assert_eq!(reqs[0].json()["listen_type"], "playing_now");
        assert_eq!(reqs[1].json()["listen_type"], "single");
        assert_eq!(reqs[2].json()["listen_type"], "import");
        assert_eq!(reqs[2].json()["payload"][1]["listened_at"], 20);
    }

    #[test]
    fn maps_error_statuses() {
        let server = MockServer::start();
        server
            .reply(401, r#"{"code":401,"error":"Invalid authorization token."}"#)
            .reply(503, "<html>down</html>")
            .reply(429, "{}")
            .reply(400, r#"{"code":400,"error":"bad listen"}"#);
        let client = ListenBrainzClient::new(&server.url, "tok");
        assert!(matches!(client.submit(&[listen(1)]), Err(SubmitError::Auth(m)) if m.contains("Invalid authorization")));
        assert!(matches!(client.submit(&[listen(1)]), Err(SubmitError::Retry(_))));
        assert!(matches!(client.submit(&[listen(1)]), Err(SubmitError::Retry(_))));
        assert!(matches!(client.submit(&[listen(1)]), Err(SubmitError::Rejected(m)) if m.contains("bad listen")));
    }

    #[test]
    fn unreachable_server_is_retried() {
        // A port that was free a moment ago: nothing listens there.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let client = ListenBrainzClient::new(&format!("http://127.0.0.1:{port}"), "tok");
        assert!(matches!(client.submit(&[listen(1)]), Err(SubmitError::Retry(_))));
    }

    #[test]
    fn validates_token() {
        let server = MockServer::start();
        server
            .reply(200, r#"{"code":200,"message":"Token valid.","valid":true,"user_name":"alice"}"#)
            .reply(200, r#"{"code":200,"message":"Token invalid.","valid":false}"#);
        let client = ListenBrainzClient::new(&server.url, "tok");
        assert_eq!(client.validate_token().unwrap(), "alice");
        assert!(matches!(client.validate_token(), Err(SubmitError::Auth(_))));
        let reqs = server.requests();
        assert_eq!(reqs[0].method, "GET");
        assert_eq!(reqs[0].path, "/1/validate-token");
        assert_eq!(reqs[0].header("Authorization"), Some("Token tok"));
    }
}
