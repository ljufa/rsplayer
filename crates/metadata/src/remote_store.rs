//! Remote libraries: music read over the network instead of a local path.
//!
//! The only backend so far is S3-compatible object storage (AWS, B2, R2,
//! `MinIO`, Garage, or a NAS folder exposed with `rclone serve s3`). Nothing is
//! mounted: the scanner lists objects and reads tags through presigned `GET`
//! URLs wrapped in [`HttpRangeSource`], and playback streams the same way.
//!
//! Songs from a remote library are stored under keys
//! `rs3:<library id>/<path below the configured prefix>`, so they never
//! collide with library-relative local keys and are never mistaken for
//! `http…` radio/podcast URLs.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, format_err};
use api_models::settings::{AWS_S3_HOST, MetadataStoreSettings, RemoteLibrary, RemoteLibraryKind};
use chrono::{DateTime, Utc};
use log::{info, warn};
use rusty_s3::actions::ListObjectsV2;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

use crate::http_range_source::HttpRangeSource;

/// Prefix of database keys of songs from a remote library.
pub const REMOTE_KEY_PREFIX: &str = "rs3:";

/// Lifetime of presigned URLs. Long enough to cover a paused track: the
/// range source reuses the URL when it reconnects or seeks.
const PRESIGN_EXPIRY: Duration = Duration::from_hours(24);
/// Lifetime of a presigned listing request (used immediately).
const LIST_EXPIRY: Duration = Duration::from_mins(10);

pub fn is_remote_key(key: &str) -> bool {
    key.starts_with(REMOTE_KEY_PREFIX)
}

pub fn remote_key(library_id: &str, path: &str) -> String {
    format!("{REMOTE_KEY_PREFIX}{library_id}/{path}")
}

/// `rs3:<id>/<path>` → `(id, path)`.
pub fn split_remote_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix(REMOTE_KEY_PREFIX)?.split_once('/')
}

/// One object of a remote library, `path` relative to the library prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteObject {
    pub path: String,
    pub size: u64,
    pub last_modified: Option<DateTime<Utc>>,
}

pub trait RemoteStore: Send + Sync {
    /// All objects of the library.
    fn list(&self) -> Result<Vec<RemoteObject>>;
    /// A URL that serves `path` with byte-range support, no extra headers needed.
    fn read_url(&self, path: &str) -> String;
}

pub fn store_for(library: &RemoteLibrary) -> Result<Box<dyn RemoteStore>> {
    match &library.kind {
        RemoteLibraryKind::S3 { .. } => Ok(Box::new(S3Store::new(&library.kind)?)),
    }
}

/// Presigned URL for the song stored under `db_key`.
pub fn resolve_url(settings: &MetadataStoreSettings, db_key: &str) -> Result<String> {
    let (library_id, path) = split_remote_key(db_key).ok_or_else(|| format_err!("Not a remote key: {db_key}"))?;
    let library = settings
        .find_remote_library(library_id)
        .ok_or_else(|| format_err!("Remote library {library_id} is not configured"))?;
    Ok(store_for(library)?.read_url(path))
}

/// Opens the song stored under `db_key` as a seekable media source.
pub fn open(settings: &MetadataStoreSettings, db_key: &str) -> Result<HttpRangeSource> {
    let url = resolve_url(settings, db_key)?;
    HttpRangeSource::open(http_agent(), &url).with_context(|| format!("Failed to open {db_key}"))
}

/// Checks credentials and reachability; returns the number of objects found.
pub fn test_connection(library: &RemoteLibrary) -> Result<usize> {
    Ok(store_for(library)?.list()?.len())
}

/// Agent for streaming: bounded connect/header waits, no body deadline (a
/// body may be a whole track being played).
fn http_agent() -> ureq::Agent {
    agent_with_response_timeout(Duration::from_secs(30))
}

/// Agent for listing. Gateways over a plain filesystem (`rclone serve s3`)
/// walk the whole tree before answering a recursive listing; on a cold
/// 20k-file NAS library that took over a minute.
fn list_agent() -> ureq::Agent {
    agent_with_response_timeout(Duration::from_mins(5))
}

fn agent_with_response_timeout(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(timeout))
        .build()
        .into()
}

pub struct S3Store {
    bucket: Bucket,
    credentials: Credentials,
    /// Normalized: no leading `/`, ends with `/` unless empty.
    prefix: String,
}

impl S3Store {
    pub fn new(kind: &RemoteLibraryKind) -> Result<Self> {
        let RemoteLibraryKind::S3 {
            endpoint,
            bucket,
            region,
            prefix,
            access_key,
            secret_key,
            path_style,
        } = kind;
        let mut endpoint: url::Url = endpoint.trim().parse().with_context(|| format!("Invalid endpoint URL: {endpoint}"))?;
        let style = if *path_style { UrlStyle::Path } else { UrlStyle::VirtualHost };
        let bucket_name = bucket.trim().to_string();
        let new_bucket = |endpoint: url::Url, region: String| {
            Bucket::new(endpoint, style, bucket_name.clone(), region).map_err(|e| format_err!("Invalid S3 bucket configuration: {e:?}"))
        };
        let mut region = region.trim().to_string();
        if region.is_empty() {
            region = detected_region(&new_bucket(endpoint.clone(), FALLBACK_REGION.to_string())?);
            // s3://bucket: the global endpoint only signs for us-east-1.
            if endpoint.host_str() == Some(AWS_S3_HOST) && region != FALLBACK_REGION {
                endpoint = format!("https://s3.{region}.amazonaws.com").parse()?;
            }
        }
        let bucket = new_bucket(endpoint, region)?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(access_key.trim(), secret_key.trim()),
            prefix: normalize_prefix(prefix),
        })
    }

    fn list_url(&self, continuation: Option<&str>) -> url::Url {
        let mut action = ListObjectsV2::new(&self.bucket, Some(&self.credentials));
        // With `encoding-type=url` servers encode spaces as `+` (form
        // encoding; rclone and AWS both do), which rusty-s3 does not decode:
        // `Keith Jarrett/…` came back as `Keith+Jarrett/…`. Plain keys in the
        // XML are unambiguous for any name a music library holds.
        action.query_mut().remove("encoding-type");
        if !self.prefix.is_empty() {
            action.with_prefix(self.prefix.as_str());
        }
        if let Some(token) = continuation {
            action.with_continuation_token(token);
        }
        action.sign(LIST_EXPIRY)
    }

    fn list_page(&self, agent: &ureq::Agent, continuation: Option<&str>) -> Result<(Vec<RemoteObject>, Option<String>)> {
        let url = self.list_url(continuation);
        let body = agent
            .get(url.as_str())
            .call()
            .map_err(|e| format_err!("Listing bucket {} failed: {e}", self.bucket.name()))?
            .body_mut()
            .read_to_string()?;
        let parsed = ListObjectsV2::parse_response(&body).map_err(|e| format_err!("Invalid listing response: {e}"))?;
        let objects = parsed
            .contents
            .into_iter()
            .filter_map(|c| {
                let path = c.key.strip_prefix(&self.prefix)?.to_string();
                // Folder placeholder objects.
                if path.is_empty() || path.ends_with('/') {
                    return None;
                }
                Some(RemoteObject {
                    path,
                    size: c.size,
                    last_modified: DateTime::parse_from_rfc3339(&c.last_modified).ok().map(|d| d.with_timezone(&Utc)),
                })
            })
            .collect();
        Ok((objects, parsed.next_continuation_token))
    }
}

impl RemoteStore for S3Store {
    fn list(&self) -> Result<Vec<RemoteObject>> {
        let agent = list_agent();
        let mut all = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let (objects, next) = self.list_page(&agent, token.as_deref())?;
            all.extend(objects);
            match next {
                Some(t) if !t.is_empty() => token = Some(t),
                _ => break,
            }
        }
        Ok(all)
    }

    fn read_url(&self, path: &str) -> String {
        let key = format!("{}{path}", self.prefix);
        self.bucket.get_object(Some(&self.credentials), &key).sign(PRESIGN_EXPIRY).to_string()
    }
}

/// Region used when the server does not name one (rclone, `MinIO`, Garage
/// accept any region).
const FALLBACK_REGION: &str = "us-east-1";

/// The bucket's region as the server reports it in `x-amz-bucket-region`
/// (AWS sends it even on 301/403 answers to an unsigned `HEAD`), else
/// [`FALLBACK_REGION`]. Cached per bucket URL: stores are created for every
/// song opened. Network errors are not cached, so a later attempt can still
/// detect it.
fn detected_region(bucket: &Bucket) -> String {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = bucket.base_url().to_string();
    if let Some(region) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return region;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let region = match agent.head(&key).call() {
        Ok(resp) => resp
            .headers()
            .get("x-amz-bucket-region")
            .and_then(|v| v.to_str().ok())
            .map_or_else(|| FALLBACK_REGION.to_string(), str::to_string),
        Err(e) => {
            warn!("Could not detect the region of {key}: {e}; using {FALLBACK_REGION}");
            return FALLBACK_REGION.to_string();
        }
    };
    info!("S3 region for {key}: {region}");
    if let Ok(mut c) = cache.lock() {
        c.insert(key, region.clone());
    }
    region
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim().trim_matches('/');
    if trimmed.is_empty() { String::new() } else { format!("{trimmed}/") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3_kind(prefix: &str) -> RemoteLibraryKind {
        RemoteLibraryKind::S3 {
            endpoint: "https://music.example.com".into(),
            bucket: "music".into(),
            region: "us-east-1".into(),
            prefix: prefix.into(),
            access_key: "key".into(),
            secret_key: "secret".into(),
            path_style: true,
        }
    }

    #[test]
    fn remote_keys_round_trip() {
        let key = remote_key("nas", "Artist/Album/01 Track.flac");
        assert_eq!(key, "rs3:nas/Artist/Album/01 Track.flac");
        assert!(is_remote_key(&key));
        assert_eq!(split_remote_key(&key), Some(("nas", "Artist/Album/01 Track.flac")));
        assert!(!is_remote_key("Artist/Album/01.flac"));
        assert_eq!(split_remote_key("Artist/x.flac"), None);
    }

    #[test]
    fn prefix_is_normalized() {
        assert_eq!(normalize_prefix(""), "");
        assert_eq!(normalize_prefix(" / "), "");
        assert_eq!(normalize_prefix("/music"), "music/");
        assert_eq!(normalize_prefix("music/lossless/"), "music/lossless/");
    }

    #[test]
    fn read_url_is_presigned_path_style_under_prefix() {
        let store = S3Store::new(&s3_kind("music")).unwrap();
        let url = store.read_url("A B/01.flac");
        assert!(url.starts_with("https://music.example.com/music/music/A%20B/01.flac?"), "{url}");
        assert!(url.contains("X-Amz-Signature="));
    }

    /// Against a real endpoint, e.g.
    /// `rclone serve s3 <root> --auth-key key,secret --addr 127.0.0.1:8333`
    /// with a `music` folder under `<root>`:
    /// `RSP_S3_TEST_ENDPOINT=http://127.0.0.1:8333 cargo test -p metadata -- --ignored live_s3`
    #[test]
    #[ignore = "needs a live S3 endpoint"]
    fn live_s3_list_and_probe() {
        use symphonia::core::formats::{FormatOptions, probe::Hint};
        use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
        use symphonia::core::meta::MetadataOptions;

        let endpoint = std::env::var("RSP_S3_TEST_ENDPOINT").expect("RSP_S3_TEST_ENDPOINT");
        let mut kind = s3_kind("");
        let RemoteLibraryKind::S3 { endpoint: ep, region, .. } = &mut kind;
        *ep = endpoint;
        // Empty: exercises region detection (rclone names none -> fallback).
        region.clear();
        let library = RemoteLibrary {
            id: "t".into(),
            name: "test".into(),
            enabled: true,
            kind,
        };
        let objects = store_for(&library).unwrap().list().unwrap();
        assert!(!objects.is_empty());
        let settings = MetadataStoreSettings {
            remote_libraries: vec![library],
            ..Default::default()
        };
        let audio = objects.iter().filter(|o| o.path.ends_with(".flac") || o.path.ends_with(".mp3"));
        let mut probed_count = 0;
        for object in audio {
            let source = open(&settings, &remote_key("t", &object.path)).unwrap_or_else(|e| panic!("{}: {e:#}", object.path));
            let mss = MediaSourceStream::new(Box::new(source), MediaSourceStreamOptions::default());
            let probed = crate::build_probe()
                .probe(&Hint::new(), mss, FormatOptions::default(), MetadataOptions::default())
                .unwrap();
            assert!(!probed.tracks().is_empty());
            probed_count += 1;
        }
        assert!(probed_count > 0, "no audio objects");
    }

    #[test]
    fn listing_asks_for_plain_keys() {
        let store = S3Store::new(&s3_kind("music")).unwrap();
        let url = store.list_url(None).to_string();
        assert!(url.contains("list-type=2"), "{url}");
        assert!(!url.contains("encoding-type"), "{url}");
    }

    #[test]
    fn invalid_endpoint_is_rejected() {
        let mut kind = s3_kind("");
        let RemoteLibraryKind::S3 { endpoint, .. } = &mut kind;
        *endpoint = "not a url".into();
        assert!(S3Store::new(&kind).is_err());
    }
}
