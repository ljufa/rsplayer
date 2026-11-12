//! Last.fm client: the desktop auth flow, `track.updateNowPlaying` and
//! `track.scrobble` (<https://www.last.fm/api/scrobbling>).
//!
//! Write calls are signed with the user's own API secret: `api_sig` is the
//! MD5 of every parameter (`format` excluded) as `name` + `value`, sorted by
//! name, followed by the secret.

use md5::{Digest, Md5};
use serde_json::Value;

use crate::http::{self, Reply, SubmitError, status_error_kind};
use crate::track::{Listen, ScrobbleTrack};

pub const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
/// Where the user approves access for a token.
pub const AUTH_URL: &str = "https://www.last.fm/api/auth/";
/// Scrobbles per `track.scrobble` request (the API maximum).
pub const BATCH_SIZE: usize = 50;

pub struct LastFmClient {
    url: String,
    key: String,
    secret: String,
}

impl LastFmClient {
    #[must_use]
    pub fn new(api_url: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            url: api_url.to_string(),
            key: api_key.trim().to_string(),
            secret: api_secret.trim().to_string(),
        }
    }

    /// Step 1 of the auth flow: a token the user then approves.
    pub fn get_token(&self) -> Result<String, SubmitError> {
        let v = self.call(params(&[("method", "auth.getToken")]))?;
        str_at(&v, &["token"]).ok_or_else(|| SubmitError::Rejected("answer has no token".into()))
    }

    /// Step 3, after the user approved `token`: `(user name, session key)`.
    pub fn get_session(&self, token: &str) -> Result<(String, String), SubmitError> {
        let v = self.call(params(&[("method", "auth.getSession"), ("token", token)]))?;
        let name = str_at(&v, &["session", "name"]);
        let key = str_at(&v, &["session", "key"]);
        name.zip(key).ok_or_else(|| SubmitError::Rejected("answer has no session".into()))
    }

    pub fn update_now_playing(&self, session_key: &str, track: &ScrobbleTrack) -> Result<(), SubmitError> {
        let mut p = params(&[("method", "track.updateNowPlaying"), ("sk", session_key)]);
        track_params(&mut p, track, None);
        self.call(p).map(|_| ())
    }

    /// Up to [`BATCH_SIZE`] listens in one request.
    pub fn scrobble(&self, session_key: &str, listens: &[Listen]) -> Result<(), SubmitError> {
        let mut p = params(&[("method", "track.scrobble"), ("sk", session_key)]);
        for (i, listen) in listens.iter().enumerate() {
            track_params(&mut p, &listen.track, Some((i, listen.listened_at)));
        }
        let v = self.call(p)?;
        let ignored = v
            .pointer("/scrobbles/@attr/ignored")
            .and_then(|n| n.as_u64().or_else(|| n.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0);
        if ignored > 0 {
            log::info!("Last.fm ignored {ignored} of {} scrobbles", listens.len());
        }
        Ok(())
    }

    /// Signs and posts a call; maps Last.fm error codes.
    fn call(&self, mut p: Vec<(String, String)>) -> Result<Value, SubmitError> {
        p.push(("api_key".into(), self.key.clone()));
        let sig = sign(&p, &self.secret);
        p.push(("api_sig".into(), sig));
        p.push(("format".into(), "json".into()));
        let reply = http::read(
            http::agent()
                .post(&self.url)
                .send_form(p.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        )?;
        interpret(&reply)
    }
}

fn interpret(reply: &Reply) -> Result<Value, SubmitError> {
    let json = reply.json();
    if let Some(v) = &json
        && let Some(code) = v.get("error").and_then(Value::as_u64)
    {
        let message = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
        return Err(error_for(code, &message));
    }
    if let Some(kind) = status_error_kind(reply.status) {
        return Err(kind(format!("HTTP {}", reply.status)));
    }
    json.ok_or_else(|| SubmitError::Retry("answer is not JSON".into()))
}

/// <https://www.last.fm/api/errorcodes>
fn error_for(code: u64, message: &str) -> SubmitError {
    let text = format!("Last.fm error {code}: {message}");
    match code {
        9 => SubmitError::InvalidSession,
        // Service offline, temporarily unavailable, rate limit exceeded.
        11 | 16 | 29 => SubmitError::Retry(text),
        // Invalid API key, signature, suspended key.
        10 | 13 | 26 => SubmitError::Auth(text),
        _ => SubmitError::Rejected(text),
    }
}

fn params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

/// Adds a track's fields; `indexed` gives the `[i]` suffix and timestamp of
/// a scrobble.
fn track_params(p: &mut Vec<(String, String)>, track: &ScrobbleTrack, indexed: Option<(usize, i64)>) {
    let name = |n: &str| indexed.map_or_else(|| n.to_string(), |(i, _)| format!("{n}[{i}]"));
    p.push((name("artist"), track.artist.clone()));
    p.push((name("track"), track.title.clone()));
    if let Some((_, ts)) = indexed {
        p.push((name("timestamp"), ts.to_string()));
    }
    if let Some(album) = &track.album {
        p.push((name("album"), album.clone()));
    }
    if let Some(a) = &track.album_artist {
        p.push((name("albumArtist"), a.clone()));
    }
    if let Some(d) = track.duration_secs {
        p.push((name("duration"), d.to_string()));
    }
    if let Some(n) = track.track_number {
        p.push((name("trackNumber"), n.to_string()));
    }
    if let Some(id) = &track.recording_mbid {
        p.push((name("mbid"), id.clone()));
    }
}

/// The `api_sig` of a call.
#[must_use]
pub fn sign(params: &[(String, String)], secret: &str) -> String {
    let mut sorted: Vec<&(String, String)> = params.iter().filter(|(k, _)| k != "format" && k != "callback").collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut hasher = Md5::new();
    for (k, v) in sorted {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    hasher.update(secret.as_bytes());
    hasher.finalize().iter().fold(String::with_capacity(32), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The page where the user grants access for `token`.
#[must_use]
pub fn auth_page_url(api_key: &str, token: &str) -> String {
    format!("{AUTH_URL}?api_key={}&token={}", http::encode(api_key), http::encode(token))
}

fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockServer;

    fn track() -> ScrobbleTrack {
        ScrobbleTrack {
            artist: "Cher".into(),
            title: "Believe".into(),
            album: Some("Believe".into()),
            duration_secs: Some(239),
            track_number: Some(1),
            ..Default::default()
        }
    }

    #[test]
    fn signature_matches_known_vector() {
        // The example from last.fm/api/authspec (section 8).
        let p = params(&[("token", "yyyyyy"), ("method", "auth.getSession"), ("api_key", "xxxxxxxxxx")]);
        assert_eq!(sign(&p, "ilovecher"), "b87d61da3cda91a8b6746c4aef55d6f8");
    }

    #[test]
    fn signature_sorts_params_and_skips_format() {
        let p = params(&[
            ("track", "Believe"),
            ("format", "json"),
            ("sk", "sss"),
            ("method", "track.updateNowPlaying"),
            ("artist", "Cher"),
            ("api_key", "kkk"),
        ]);
        // md5("api_keykkkartistChermethodtrack.updateNowPlayingskssstrackBelievesecret")
        assert_eq!(sign(&p, "secret"), "11837a45e0473827a3ef954f61d891b3");
    }

    #[test]
    fn auth_url_carries_key_and_token() {
        assert_eq!(
            auth_page_url("key", "to ken"),
            "https://www.last.fm/api/auth/?api_key=key&token=to%20ken"
        );
    }

    #[test]
    fn now_playing_request_is_signed_form() {
        let server = MockServer::start();
        let client = LastFmClient::new(&server.url, "kkk", "secret");
        client.update_now_playing("sss", &track()).unwrap();
        let req = &server.requests()[0];
        assert_eq!(req.method, "POST");
        assert_eq!(req.field("method").as_deref(), Some("track.updateNowPlaying"));
        assert_eq!(req.field("artist").as_deref(), Some("Cher"));
        assert_eq!(req.field("track").as_deref(), Some("Believe"));
        assert_eq!(req.field("duration").as_deref(), Some("239"));
        assert_eq!(req.field("trackNumber").as_deref(), Some("1"));
        assert_eq!(req.field("sk").as_deref(), Some("sss"));
        assert_eq!(req.field("format").as_deref(), Some("json"));
        assert!(req.field("timestamp").is_none());
        let unsigned: Vec<(String, String)> = req.form().into_iter().filter(|(k, _)| k != "api_sig").collect();
        assert_eq!(req.field("api_sig"), Some(sign(&unsigned, "secret")));
    }

    #[test]
    fn scrobble_batch_uses_indexed_params() {
        let server = MockServer::start();
        server.reply(200, r#"{"scrobbles":{"@attr":{"accepted":2,"ignored":0}}}"#);
        let client = LastFmClient::new(&server.url, "kkk", "secret");
        let listens = [
            Listen { track: track(), listened_at: 100 },
            Listen {
                track: ScrobbleTrack {
                    artist: "ABBA".into(),
                    title: "SOS".into(),
                    ..Default::default()
                },
                listened_at: 200,
            },
        ];
        client.scrobble("sss", &listens).unwrap();
        let req = &server.requests()[0];
        assert_eq!(req.field("method").as_deref(), Some("track.scrobble"));
        assert_eq!(req.field("artist[0]").as_deref(), Some("Cher"));
        assert_eq!(req.field("timestamp[0]").as_deref(), Some("100"));
        assert_eq!(req.field("artist[1]").as_deref(), Some("ABBA"));
        assert_eq!(req.field("timestamp[1]").as_deref(), Some("200"));
        assert!(req.field("album[1]").is_none());
    }

    #[test]
    fn maps_error_codes() {
        let server = MockServer::start();
        server
            .reply(403, r#"{"error":9,"message":"Invalid session key"}"#)
            .reply(200, r#"{"error":29,"message":"Rate limit exceeded"}"#)
            .reply(503, r#"{"error":16,"message":"Try again"}"#)
            .reply(200, r#"{"error":11,"message":"Offline"}"#)
            .reply(403, r#"{"error":13,"message":"Invalid method signature"}"#)
            .reply(400, r#"{"error":6,"message":"Invalid parameters"}"#)
            .reply(502, "<html>bad gateway</html>");
        let client = LastFmClient::new(&server.url, "kkk", "secret");
        let l = [Listen { track: track(), listened_at: 1 }];
        assert_eq!(client.scrobble("sss", &l), Err(SubmitError::InvalidSession));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Retry(_))));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Retry(_))));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Retry(_))));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Auth(_))));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Rejected(_))));
        assert!(matches!(client.scrobble("sss", &l), Err(SubmitError::Retry(_))));
    }

    #[test]
    fn auth_flow() {
        let server = MockServer::start();
        server
            .reply(200, r#"{"token":"tok123"}"#)
            .reply(200, r#"{"error":14,"message":"Unauthorized Token"}"#)
            .reply(200, r#"{"session":{"name":"alice","key":"sk-1","subscriber":0}}"#);
        let client = LastFmClient::new(&server.url, "kkk", "secret");
        assert_eq!(client.get_token().unwrap(), "tok123");
        assert!(matches!(client.get_session("tok123"), Err(SubmitError::Rejected(m)) if m.contains("14")));
        assert_eq!(client.get_session("tok123").unwrap(), ("alice".into(), "sk-1".into()));
        let reqs = server.requests();
        assert_eq!(reqs[0].field("method").as_deref(), Some("auth.getToken"));
        assert_eq!(reqs[2].field("method").as_deref(), Some("auth.getSession"));
        assert_eq!(reqs[2].field("token").as_deref(), Some("tok123"));
        assert!(reqs[2].field("api_sig").is_some());
    }
}
