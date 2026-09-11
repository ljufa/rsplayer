//! Source resolution: turns a queue item's key into a probed media source.
//!
//! Local paths are resolved against the configured music directories;
//! HTTP(S) URLs are fetched with `Icy-Metadata: 1` and `Range: bytes=0-`:
//! genuine ICY radio streams are wrapped in [`IcyMetadataReader`] so title
//! updates flow out as events, hosts that honour byte ranges (podcast
//! episodes, direct file links) become a seekable [`HttpRangeSource`], and
//! anything else is a plain non-seekable stream. APE and SACD-ISO keys
//! (`…#SACD_<n>`) get their special readers.

use std::path::{Path, PathBuf};

use anyhow::{Result, format_err};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSource, ReadOnlySource};
use tokio::sync::broadcast::Sender;

use api_models::state::StateChangeEvent;
use log::info;
use metadata::icy_reader::IcyMetadataReader;
use metadata::radio_meta::{self, RadioMeta};

use crate::rsp::http_range_source::HttpRangeSource;

/// HTTP agent for long-lived audio bodies: bounded connect/response-header
/// waits, but no global or per-call deadline — those also cover body reads
/// and would cut a stream after the configured time.
pub fn build_stream_agent() -> ureq::Agent {
    use std::time::Duration;
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(10)))
        .build()
        .into()
}

/// Opens `url` and returns the media source plus, for genuine ICY radio
/// streams only, the station metadata parsed from the response headers.
pub fn probe_http_source(
    url: &str,
    hint: &mut Hint,
    changes_tx: &Sender<StateChangeEvent>,
) -> Result<(Box<dyn MediaSource>, Option<RadioMeta>)> {
    let agent = build_stream_agent();

    let resp = agent
        .get(url)
        .header("accept", "*/*")
        .header("Icy-Metadata", "1")
        .header("Range", "bytes=0-")
        .call()
        .map_err(|e| format_err!("Failed to get url {url}: {e}"))?;

    let status = resp.status().as_u16();
    info!(
        "response status code:{status} / status text:{}",
        resp.status().canonical_reason().unwrap_or("")
    );
    resp.headers()
        .iter()
        .for_each(|(name, value)| info!("{name} = {:?}", value.to_str().unwrap_or("")));

    let header_present = |name: &str| resp.headers().contains_key(name);
    let is_icy_stream = header_present("icy-metaint") || header_present("icy-name");
    let radio_meta = if is_icy_stream {
        radio_meta::get_external_radio_meta(&agent, &resp)
    } else {
        None
    };

    let ct_str = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ext = match ct_str.split(';').next().unwrap_or("").trim() {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/aac" | "audio/aacp" | "audio/x-aac" => Some("aac"),
        "audio/ogg" | "application/ogg" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        _ => None,
    };
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }

    if status != 200 && status != 206 {
        return Err(format_err!("Invalid streaming url {url}"));
    }

    let metaint_val = resp
        .headers()
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());

    let media_source: Box<dyn MediaSource> = if let (Some(metaint_val), Some(rm)) = (metaint_val, radio_meta.clone()) {
        info!("ICY stream detected with metaint={metaint_val}");
        let reader = resp.into_body().into_reader();
        let icy_reader = IcyMetadataReader::new(reader, metaint_val, changes_tx.clone(), rm);
        Box::new(ReadOnlySource::new(Box::new(icy_reader)))
    } else if is_icy_stream {
        Box::new(ReadOnlySource::new(resp.into_body().into_reader()))
    } else {
        match HttpRangeSource::try_from_response(agent, url, resp) {
            Ok(range_source) => {
                info!("Seekable HTTP source ({} bytes)", range_source.byte_len().unwrap_or(0));
                Box::new(range_source)
            }
            Err(resp) => {
                info!("HTTP source without range support, playing as a stream");
                Box::new(ReadOnlySource::new((*resp).into_body().into_reader()))
            }
        }
    };

    Ok((media_source, radio_meta))
}

pub fn probe_local_file(path_str: &str, music_dirs: &[String], hint: &mut Hint) -> Result<(Box<dyn MediaSource>, Option<RadioMeta>)> {
    for dir in music_dirs {
        let path = Path::new(dir).join(path_str);
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(&extension.to_lowercase());
        }
        if let Ok(file) = std::fs::File::open(&path) {
            return Ok((Box::new(file) as Box<dyn MediaSource>, None));
        }
    }
    Err(format_err!("Unable to open file: {path_str}"))
}

/// Resolve an APE file path across music directories.
/// Returns the full path if found, None otherwise.
pub fn resolve_ape_path(path_str: &str, music_dirs: &[String]) -> Option<PathBuf> {
    for dir in music_dirs {
        let path = Path::new(dir).join(path_str);
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && ext.eq_ignore_ascii_case("ape")
            && path.exists()
        {
            return Some(path);
        }
    }
    None
}

pub fn is_http_stream(path: &str) -> bool {
    path.starts_with("http")
}

/// If `path_str` contains the SACD virtual-track marker `#SACD_NNNN`, resolve the ISO file
/// across music directories and return `(full_iso_path, track_idx)`.
pub fn resolve_sacd_iso_path(path_str: &str, music_dirs: &[String]) -> Option<(PathBuf, usize)> {
    const MARKER: &str = metadata::sacd_bundle::SACD_TRACK_MARKER;
    let marker_pos = path_str.find(MARKER)?;
    let iso_rel = &path_str[..marker_pos];
    let track_idx: usize = path_str[marker_pos + MARKER.len()..].parse().ok()?;

    for dir in music_dirs {
        let iso_path = Path::new(dir).join(iso_rel);
        if iso_path.exists() {
            return Some((iso_path, track_idx));
        }
    }
    None
}
