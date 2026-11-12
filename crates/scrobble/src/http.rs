//! Shared HTTP plumbing of the two clients: one blocking `ureq` agent with
//! timeouts, and the error kinds the retry logic acts on.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use ureq::Agent;

/// Whole request, connect to last byte. Scrobbling is never urgent.
const TIMEOUT: Duration = Duration::from_secs(15);
/// Service answers are small JSON documents.
const RESPONSE_LIMIT_BYTES: u64 = 256 * 1024;

/// Why a call failed, which decides what happens to the scrobble.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    /// Network trouble, server error or rate limit: keep it, try again later.
    Retry(String),
    /// Credentials rejected (token, API key, signature): keep it, the user
    /// has to fix the settings.
    Auth(String),
    /// Last.fm session revoked or expired: the user has to connect again.
    InvalidSession,
    /// The service refused this request for good: drop it.
    Rejected(String),
}

impl SubmitError {
    /// Whether the scrobble should stay in the retry queue.
    #[must_use]
    pub const fn keep(&self) -> bool {
        !matches!(self, Self::Rejected(_))
    }
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retry(m) => write!(f, "temporarily unavailable: {m}"),
            Self::Auth(m) => write!(f, "credentials rejected: {m}"),
            Self::InvalidSession => write!(f, "session is no longer valid"),
            Self::Rejected(m) => write!(f, "rejected: {m}"),
        }
    }
}

impl std::error::Error for SubmitError {}

pub fn agent() -> Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| {
            Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .user_agent(format!("RSPlayer/{}", env!("CARGO_PKG_VERSION")))
                .http_status_as_error(false)
                .build()
                .into()
        })
        .clone()
}

/// Status and body of an answer.
pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.body).ok()
    }
}

/// Reads the answer of a request; a transport failure is worth a retry.
pub fn read(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply, SubmitError> {
    let resp = result.map_err(|e| SubmitError::Retry(e.to_string()))?;
    let status = resp.status().as_u16();
    let body = resp
        .into_body()
        .into_with_config()
        .limit(RESPONSE_LIMIT_BYTES)
        .read_to_string()
        .map_err(|e| SubmitError::Retry(format!("read answer: {e}")))?;
    Ok(Reply { status, body })
}

/// The kind of failure an HTTP status means, `None` for success.
pub const fn status_error_kind(status: u16) -> Option<fn(String) -> SubmitError> {
    match status {
        200..=299 => None,
        401 | 403 => Some(SubmitError::Auth),
        408 | 429 | 500..=599 => Some(SubmitError::Retry),
        _ => Some(SubmitError::Rejected),
    }
}

/// Percent-encodes a query parameter value.
pub fn encode(value: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
pub mod mock {
    //! A tiny HTTP/1.1 server on localhost for client tests: records every
    //! request and answers with queued canned replies.

    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    pub struct Recorded {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Recorded {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }

        pub fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body).expect("request body is JSON")
        }

        /// Decoded `application/x-www-form-urlencoded` fields.
        pub fn form(&self) -> Vec<(String, String)> {
            self.body
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|pair| {
                    let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                    (decode(k), decode(v))
                })
                .collect()
        }

        pub fn field(&self, name: &str) -> Option<String> {
            self.form().into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
        }
    }

    fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                    out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                    i += 2;
                }
                b => out.push(b),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub struct MockServer {
        pub url: String,
        requests: Arc<Mutex<Vec<Recorded>>>,
        replies: Arc<Mutex<VecDeque<(u16, String)>>>,
    }

    impl MockServer {
        /// Answers `200 {}` once the queued replies are used up.
        pub fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
            let url = format!("http://{}", listener.local_addr().expect("addr"));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let replies: Arc<Mutex<VecDeque<(u16, String)>>> = Arc::new(Mutex::new(VecDeque::new()));
            let (req_c, rep_c) = (requests.clone(), replies.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let Some(recorded) = read_request(&mut stream) else { continue };
                    req_c.lock().unwrap().push(recorded);
                    let (status, body) = rep_c.lock().unwrap().pop_front().unwrap_or((200, "{}".into()));
                    let answer = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(answer.as_bytes());
                }
            });
            Self { url, requests, replies }
        }

        pub fn reply(&self, status: u16, body: &str) -> &Self {
            self.replies.lock().unwrap().push_back((status, body.to_string()));
            self
        }

        pub fn requests(&self) -> Vec<Recorded> {
            self.requests.lock().unwrap().clone()
        }
    }

    fn read_request(stream: &mut std::net::TcpStream) -> Option<Recorded> {
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).ok()?;
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        let len: usize = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; len];
        reader.read_exact(&mut body).ok()?;
        Some(Recorded {
            method,
            path,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode("abc-_.~09"), "abc-_.~09");
    }

    #[test]
    fn status_kinds() {
        assert!(status_error_kind(200).is_none());
        assert_eq!(status_error_kind(401).map(|k| k(String::new())), Some(SubmitError::Auth(String::new())));
        assert_eq!(status_error_kind(503).map(|k| k(String::new())), Some(SubmitError::Retry(String::new())));
        assert_eq!(status_error_kind(429).map(|k| k(String::new())), Some(SubmitError::Retry(String::new())));
        assert_eq!(status_error_kind(400).map(|k| k(String::new())), Some(SubmitError::Rejected(String::new())));
    }
}
