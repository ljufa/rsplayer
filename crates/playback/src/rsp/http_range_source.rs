//! Seekable HTTP media source built on `Range` requests.
//!
//! Podcast episodes and other direct file URLs are served by hosts that
//! support byte ranges. Wrapping them in this source (instead of
//! `ReadOnlySource`) lets Symphonia report a real duration and honour user
//! seeks. A seek drops the open body; the next read re-requests from the new
//! offset. Short forward seeks are served by discarding bytes from the open
//! body so demuxer probing (ID3 tail, MP4 `moov`, Xing) does not open a
//! connection per hop. Dropped connections are reopened transparently at the
//! current offset.

use std::io::{self, Read, Seek, SeekFrom};

use log::{debug, warn};
use symphonia::core::io::MediaSource;
use ureq::Agent;
use ureq::Body;
use ureq::http::{HeaderMap, Response};

/// Forward seeks up to this distance are served by reading and discarding
/// from the open connection rather than reopening it.
const FORWARD_SKIP_LIMIT: u64 = 512 * 1024;
/// Consecutive failed reopen attempts before a read error is surfaced.
const MAX_REOPEN_ATTEMPTS: u32 = 3;

type BodyReader = Box<dyn Read + Send + Sync>;

pub struct HttpRangeSource {
    agent: Agent,
    url: String,
    len: u64,
    pos: u64,
    body: Option<BodyReader>,
}

impl HttpRangeSource {
    /// Wraps `resp` (the answer to a `GET` carrying `Range: bytes=0-`) when
    /// the server proved it supports byte ranges and disclosed the total
    /// length. Returns the response back otherwise so the caller can fall
    /// back to a plain streaming source.
    pub fn try_from_response(agent: Agent, url: &str, resp: Response<Body>) -> Result<Self, Box<Response<Body>>> {
        let Some(len) = seekable_length(resp.status().as_u16(), resp.headers()) else {
            return Err(Box::new(resp));
        };
        debug!("HTTP source supports byte ranges, total length {len} bytes");
        Ok(Self {
            agent,
            url: url.to_string(),
            len,
            pos: 0,
            body: Some(Box::new(resp.into_body().into_reader())),
        })
    }

    fn open_at(&self, offset: u64) -> io::Result<BodyReader> {
        debug!("Opening {} at byte offset {offset}", self.url);
        let resp = self
            .agent
            .get(&self.url)
            .header("accept", "*/*")
            .header("Range", format!("bytes={offset}-"))
            .call()
            .map_err(io::Error::other)?;
        let status = resp.status().as_u16();
        match status {
            206 => Ok(Box::new(resp.into_body().into_reader())),
            200 if offset == 0 => Ok(Box::new(resp.into_body().into_reader())),
            _ => Err(io::Error::other(format!(
                "server ignored range request at offset {offset} (status {status})"
            ))),
        }
    }
}

/// Total length of the resource when the response proves range support.
///
/// * `206 Partial Content` with `Content-Range: bytes 0-N/TOTAL`.
/// * `200 OK` with `Content-Length` and `Accept-Ranges: bytes`.
fn seekable_length(status: u16, headers: &HeaderMap) -> Option<u64> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let len = match status {
        206 => header("content-range").and_then(parse_content_range_total),
        200 => {
            let accepts_ranges = header("accept-ranges").is_some_and(|v| v.eq_ignore_ascii_case("bytes"));
            if accepts_ranges {
                header("content-length").and_then(|v| v.trim().parse().ok())
            } else {
                None
            }
        }
        _ => None,
    }?;
    (len > 0).then_some(len)
}

/// `bytes 0-1023/4096` → `Some(4096)`; an unknown total (`*`) yields `None`.
fn parse_content_range_total(value: &str) -> Option<u64> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (_, total) = rest.rsplit_once('/')?;
    total.trim().parse().ok()
}

impl Read for HttpRangeSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let mut attempts = 0;
        loop {
            if self.body.is_none() {
                match self.open_at(self.pos) {
                    Ok(body) => self.body = Some(body),
                    Err(e) => {
                        attempts += 1;
                        if attempts > MAX_REOPEN_ATTEMPTS {
                            return Err(e);
                        }
                        warn!("Reopening HTTP source failed ({attempts}/{MAX_REOPEN_ATTEMPTS}): {e}");
                        continue;
                    }
                }
            }
            let body = self.body.as_mut().expect("body opened above");
            match body.read(buf) {
                Ok(0) => {
                    // Premature end of body: the server closed early, retry from here.
                    self.body = None;
                    attempts += 1;
                    if attempts > MAX_REOPEN_ATTEMPTS {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            format!("connection closed at {} of {} bytes", self.pos, self.len),
                        ));
                    }
                    warn!("HTTP body ended early at {} of {} bytes, reconnecting", self.pos, self.len);
                }
                Ok(n) => {
                    self.pos += n as u64;
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.body = None;
                    attempts += 1;
                    if attempts > MAX_REOPEN_ATTEMPTS {
                        return Err(e);
                    }
                    warn!("HTTP read failed at byte {} ({attempts}/{MAX_REOPEN_ATTEMPTS}): {e}", self.pos);
                }
            }
        }
    }
}

impl Seek for HttpRangeSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(offset) => self.len.checked_add_signed(offset),
            SeekFrom::Current(offset) => self.pos.checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start of resource"))?;

        if target == self.pos {
            return Ok(self.pos);
        }
        if self.body.is_some() && target > self.pos && target - self.pos <= FORWARD_SKIP_LIMIT {
            let mut scratch = [0u8; 16 * 1024];
            while self.pos < target {
                #[allow(clippy::cast_possible_truncation)]
                let want = ((target - self.pos).min(scratch.len() as u64)) as usize;
                match self.read(&mut scratch[..want]) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }
            if self.pos == target {
                return Ok(self.pos);
            }
        }
        self.body = None;
        self.pos = target;
        Ok(self.pos)
    }
}

impl MediaSource for HttpRangeSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    /// Minimal HTTP/1.1 server serving one byte blob with `Range` support.
    /// `drop_after` truncates every body to that many bytes to simulate a
    /// server that closes the connection early.
    struct FakeServer {
        url: String,
        requests: Arc<AtomicUsize>,
    }

    fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                ureq::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                ureq::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    fn start_server(data: Arc<Vec<u8>>, ranges: bool, drop_after: Option<usize>) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                let data = data.clone();
                thread::spawn(move || handle(stream, &data, ranges, drop_after));
            }
        });
        FakeServer {
            url: format!("http://{addr}/episode.bin"),
            requests,
        }
    }

    fn handle(mut stream: TcpStream, data: &[u8], ranges: bool, drop_after: Option<usize>) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        let mut range: Option<u64> = None;
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                range = v.trim().trim_end_matches('-').parse().ok();
            }
        }
        let total = data.len();
        let (status, start) = match range {
            Some(start) if ranges && (start as usize) < total => (206, start as usize),
            _ => (200, 0),
        };
        let mut body = &data[start..];
        if let Some(limit) = drop_after {
            body = &body[..body.len().min(limit)];
        }
        let mut head = format!("HTTP/1.1 {status} OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n");
        if status == 206 {
            head.push_str(&format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1));
        } else if ranges {
            head.push_str("Accept-Ranges: bytes\r\n");
        }
        // Advertise the full remaining length even when truncating, so the
        // client sees a premature EOF rather than a clean end.
        head.push_str(&format!("Content-Length: {}\r\n\r\n", total - start));
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    }

    fn open(server: &FakeServer) -> Result<HttpRangeSource, Box<Response<Body>>> {
        let agent: Agent = Agent::config_builder().http_status_as_error(false).build().into();
        let resp = agent.get(&server.url).header("Range", "bytes=0-").call().unwrap();
        HttpRangeSource::try_from_response(agent, &server.url, resp)
    }

    fn blob(len: usize) -> Arc<Vec<u8>> {
        Arc::new((0..len).map(|i| (i % 251) as u8).collect())
    }

    #[test]
    fn decision_table() {
        assert_eq!(seekable_length(206, &header_map(&[("content-range", "bytes 0-9/4096")])), Some(4096));
        assert_eq!(seekable_length(206, &header_map(&[("content-range", "bytes 0-9/*")])), None);
        assert_eq!(
            seekable_length(200, &header_map(&[("content-length", "77"), ("accept-ranges", "bytes")])),
            Some(77)
        );
        assert_eq!(seekable_length(200, &header_map(&[("content-length", "77")])), None);
        assert_eq!(
            seekable_length(200, &header_map(&[("content-length", "77"), ("accept-ranges", "none")])),
            None
        );
        assert_eq!(seekable_length(200, &header_map(&[("icy-metaint", "16000")])), None);
        assert_eq!(seekable_length(404, &header_map(&[("content-range", "bytes 0-9/4096")])), None);
    }

    #[test]
    fn reads_whole_resource_sequentially() {
        let data = blob(100_000);
        let server = start_server(data.clone(), true, None);
        let mut src = open(&server).expect("range support detected");
        assert_eq!(src.byte_len(), Some(100_000));
        let mut out = Vec::new();
        src.read_to_end(&mut out).unwrap();
        assert_eq!(out, *data);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn falls_back_when_server_ignores_ranges() {
        let server = start_server(blob(1000), false, None);
        assert!(open(&server).is_err());
    }

    #[test]
    fn seek_far_reopens_at_offset() {
        let data = blob(2_000_000);
        let server = start_server(data.clone(), true, None);
        let mut src = open(&server).unwrap();
        let pos = src.seek(SeekFrom::Start(1_500_000)).unwrap();
        assert_eq!(pos, 1_500_000);
        let mut buf = [0u8; 16];
        src.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[1_500_000..1_500_016]);
        assert_eq!(src.stream_position().unwrap(), 1_500_016);
        assert_eq!(server.requests.load(Ordering::SeqCst), 2);

        // ~500 KiB ahead: within the forward-skip window, no new connection.
        let pos = src.seek(SeekFrom::End(-8)).unwrap();
        assert_eq!(pos, 1_999_992);
        let mut tail = Vec::new();
        src.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, data[1_999_992..]);
        assert_eq!(server.requests.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn short_forward_seek_reuses_connection() {
        let data = blob(300_000);
        let server = start_server(data.clone(), true, None);
        let mut src = open(&server).unwrap();
        let mut buf = [0u8; 4];
        src.read_exact(&mut buf).unwrap();
        src.seek(SeekFrom::Current(100_000)).unwrap();
        src.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[100_004..100_008]);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn backward_seek_reopens() {
        let data = blob(50_000);
        let server = start_server(data.clone(), true, None);
        let mut src = open(&server).unwrap();
        src.seek(SeekFrom::Start(40_000)).unwrap();
        let mut buf = [0u8; 4];
        src.read_exact(&mut buf).unwrap();
        // The first seek was a short forward skip on the open connection;
        // only the backward seek forces a reopen.
        src.seek(SeekFrom::Start(10)).unwrap();
        src.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data[10..14]);
        assert_eq!(server.requests.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn reconnects_after_premature_close() {
        let data = blob(120_000);
        // Every response is cut after 50 000 bytes; the source must resume.
        let server = start_server(data.clone(), true, Some(50_000));
        let mut src = open(&server).unwrap();
        let mut out = Vec::new();
        src.read_to_end(&mut out).unwrap();
        assert_eq!(out, *data);
        assert_eq!(server.requests.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn seek_before_start_is_rejected() {
        let server = start_server(blob(10), true, None);
        let mut src = open(&server).unwrap();
        assert!(src.seek(SeekFrom::Current(-1)).is_err());
        assert_eq!(src.stream_position().unwrap(), 0);
    }
}
