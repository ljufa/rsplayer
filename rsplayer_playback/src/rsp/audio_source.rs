use std::path::{Path, PathBuf};

use anyhow::{format_err, Result};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSource, ReadOnlySource};
use tokio::sync::broadcast::Sender;

use api_models::state::StateChangeEvent;
use log::info;
use rsplayer_metadata::icy_reader::IcyMetadataReader;
use rsplayer_metadata::radio_meta::{self, RadioMeta};

pub fn probe_http_source(
    url: &str,
    hint: &mut Hint,
    changes_tx: &Sender<StateChangeEvent>,
) -> Result<(Box<dyn MediaSource>, Option<RadioMeta>)> {
    use std::time::Duration;

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(3))
        .timeout_write(Duration::from_secs(3))
        .build();

    let resp = agent
        .get(url)
        .set("accept", "*/*")
        .set("Icy-Metadata", "1")
        .call()
        .map_err(|e| format_err!("Failed to get url {url}: {e}"))?;

    let status = resp.status();
    info!("response status code:{status} / status text:{}", resp.status_text());
    resp.headers_names()
        .iter()
        .for_each(|header| info!("{header} = {:?}", resp.header(header).unwrap_or("")));

    let radio_meta = radio_meta::get_external_radio_meta(&agent, &resp);

    if let Some(ct) = resp.header("content-type") {
        let ext = match ct {
            "audio/mpeg" => Some("mp3"),
            "audio/aac" | "audio/aacp" | "audio/x-aac" => Some("aac"),
            "audio/ogg" | "application/ogg" => Some("ogg"),
            "audio/flac" | "audio/x-flac" => Some("flac"),
            "audio/wav" | "audio/x-wav" => Some("wav"),
            "audio/mp4" | "audio/x-m4a" => Some("m4a"),
            _ => None,
        };
        if let Some(ext) = ext {
            hint.with_extension(ext);
        }
    }

    if status != 200 {
        return Err(format_err!("Invalid streaming url {url}"));
    }

    let media_source: Box<dyn MediaSource> = if let Some(metaint_str) = resp.header("icy-metaint") {
        if let Ok(metaint_val) = metaint_str.parse::<usize>() {
            info!("ICY stream detected with metaint={metaint_val}");
            let reader = resp.into_reader();
            let icy_reader =
                IcyMetadataReader::new(reader, metaint_val, changes_tx.clone(), radio_meta.clone().unwrap());
            Box::new(ReadOnlySource::new(Box::new(icy_reader)))
        } else {
            Box::new(ReadOnlySource::new(resp.into_reader()))
        }
    } else {
        Box::new(ReadOnlySource::new(resp.into_reader()))
    };

    Ok((media_source, radio_meta))
}

pub fn probe_local_file(
    path_str: &str,
    music_dirs: &[String],
    hint: &mut Hint,
) -> Result<(Box<dyn MediaSource>, Option<RadioMeta>)> {
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
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("ape") && path.exists() {
                return Some(path);
            }
        }
    }
    None
}

pub fn is_http_stream(path: &str) -> bool {
    path.starts_with("http")
}
