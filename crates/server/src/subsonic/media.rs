//! `stream`, `download` and `getCoverArt`.
//!
//! Songs are sent as the original file, whatever `format` or `maxBitRate`
//! asks for: no transcoding. Unlike `/music/` for the web UI, which answers
//! every request with one bounded chunk, a request gets everything it asks
//! for (the whole file, or the whole range), streamed, since download-style
//! clients read the body once. Remote-library songs redirect to a presigned
//! storage URL; cue sheet tracks are decoded and sent as WAV.

use std::path::{Path, PathBuf};

use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures::stream;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use metadata::cue_bundle::wav::WavTrack;

use super::ids::{self, Kind};
use super::library::{content_type, suffix_of};
use super::response::{self, ApiError, Format};
use super::{Params, SubsonicState};

const READ_CHUNK: u64 = 256 * 1024;
const ARTWORK_DIR: &str = "artwork";

pub async fn stream(state: &SubsonicState, p: &Params, headers: &HeaderMap, format: &Format) -> Response {
    let Some(id) = p.get("id") else {
        return response::render(format, &Err(ApiError::missing("id")));
    };
    let key = ids::decode_as(Kind::Song, id).filter(|k| !k.contains(".."));
    let Some(key) = key.filter(|k| state.browse.song_repository.find_by_id(k).is_some()) else {
        return response::render(format, &Err(ApiError::not_found("Song")));
    };
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let settings = state.config.get_settings().metadata_settings;

    if metadata::remote_store::is_remote_key(&key) {
        return match metadata::remote_store::resolve_url(&settings, &key) {
            Ok(url) => (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, url)]).into_response(),
            Err(e) => {
                log::warn!("Subsonic: cannot resolve remote song {key}: {e}");
                response::render(format, &Err(ApiError::not_found("Song")))
            }
        };
    }
    let music_dirs = settings.effective_directories();
    if metadata::cue_bundle::is_cue_key(&key) {
        return stream_cue_track(key, range, music_dirs, format).await;
    }
    let file = music_dirs.iter().map(|dir| PathBuf::from(dir).join(&key)).find(|p| p.is_file());
    match file {
        Some(path) => stream_file(&path, &suffix_of(&key), range.as_deref()).await,
        None => response::render(format, &Err(ApiError::not_found("File"))),
    }
}

/// `start..=end` of the requested `range` (the whole file without one),
/// `Err` with the 416 response when it cannot be served.
fn byte_range(range: Option<&str>, size: u64) -> Result<(u64, u64, bool), Box<Response>> {
    range.map_or(Ok((0, size.saturating_sub(1), false)), |r| {
        crate::server::parse_range(r, size).map(|(a, b)| (a, b, true)).ok_or_else(|| {
            Box::new(
                Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                    .body(Body::empty())
                    .unwrap_or_default(),
            )
        })
    })
}

fn audio_response(content_type: &str, start: u64, end: u64, size: u64, partial: bool, body: Body) -> Response {
    let mut builder = Response::builder()
        .status(if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, (end - start + 1).to_string());
    if partial {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    builder.body(body).unwrap_or_default()
}

async fn stream_file(path: &Path, suffix: &str, range: Option<&str>) -> Response {
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let size = meta.len();
    if size == 0 {
        return Response::builder()
            .header(header::CONTENT_TYPE, content_type(suffix))
            .header(header::CONTENT_LENGTH, "0")
            .body(Body::empty())
            .unwrap_or_default();
    }
    let (start, end, partial) = match byte_range(range, size) {
        Ok(r) => r,
        Err(resp) => return *resp,
    };
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let body = stream::unfold((file, end - start + 1), |(mut file, remaining)| async move {
        if remaining == 0 {
            return None;
        }
        // Bounded by READ_CHUNK, so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation)]
        let mut buf = vec![0u8; remaining.min(READ_CHUNK) as usize];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(Bytes::from(buf)), (file, remaining - n as u64)))
            }
            Err(e) => Some((Err(e), (file, 0))),
        }
    });
    audio_response(content_type(suffix), start, end, size, partial, Body::from_stream(body))
}

/// A cue track decoded to WAV, read chunk by chunk on blocking threads.
async fn stream_cue_track(key: String, range: Option<String>, music_dirs: Vec<String>, format: &Format) -> Response {
    let opened = tokio::task::spawn_blocking(move || WavTrack::open(&key, &music_dirs)).await;
    let track = match opened {
        Ok(Ok(track)) => track,
        Ok(Err(e)) => {
            log::warn!("Subsonic: cannot open cue track: {e:#}");
            return response::render(format, &Err(ApiError::not_found("Song")));
        }
        Err(e) => {
            log::warn!("Subsonic: cue track task failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let size = track.total_size();
    let (start, end, partial) = match byte_range(range.as_deref(), size) {
        Ok(r) => r,
        Err(resp) => return *resp,
    };
    let body = stream::unfold((Some(track), start), move |(track, pos)| async move {
        let mut track = track?;
        if pos > end {
            return None;
        }
        let chunk_end = end.min(pos + READ_CHUNK - 1);
        let read = tokio::task::spawn_blocking(move || {
            let bytes = track.read_range(pos, chunk_end);
            (track, bytes)
        })
        .await;
        match read {
            Ok((track, Ok(bytes))) if !bytes.is_empty() => {
                let next = pos + bytes.len() as u64;
                Some((Ok(Bytes::from(bytes)), (Some(track), next)))
            }
            Ok((_, Ok(_))) => None,
            Ok((_, Err(e))) => Some((Err(std::io::Error::other(e.to_string())), (None, pos))),
            Err(e) => Some((Err(std::io::Error::other(e.to_string())), (None, pos))),
        }
    });
    audio_response("audio/wav", start, end, size, partial, Body::from_stream(body))
}

/// The artwork image id behind a cover art, album, song or artist id.
fn image_id_for(state: &SubsonicState, id: &str) -> Option<String> {
    let image = match ids::decode(id)? {
        (Kind::CoverArt, image) => Some(image),
        (Kind::Album, key) => state.browse.album_repository.find_by_id(&key).and_then(|a| a.image_id),
        (Kind::Song, key) => state.browse.song_repository.find_by_id(&key).and_then(|s| s.image_id),
        (Kind::Artist, name) => state
            .browse
            .album_repository
            .find_by_artist(&name)
            .into_iter()
            .find_map(|a| a.image_id),
        (Kind::Playlist, _) => None,
    }?;
    // A plain file name inside the artwork folder, nothing else.
    let safe = !image.is_empty() && !image.contains(['/', '\\']) && !image.contains("..");
    safe.then_some(image)
}

fn image_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

/// The stored artwork as is; `size` is ignored.
pub async fn cover_art(state: &SubsonicState, p: &Params, format: &Format) -> Response {
    let Some(id) = p.get("id").map(str::to_owned) else {
        return response::render(format, &Err(ApiError::missing("id")));
    };
    let lookup_state = state.clone();
    let image = tokio::task::spawn_blocking(move || image_id_for(&lookup_state, &id))
        .await
        .ok()
        .flatten();
    let Some(image) = image else {
        return response::render(format, &Err(ApiError::not_found("Cover art")));
    };
    tokio::fs::read(Path::new(ARTWORK_DIR).join(&image)).await.map_or_else(
        |_| response::render(format, &Err(ApiError::not_found("Cover art"))),
        |bytes| {
            Response::builder()
                .header(header::CONTENT_TYPE, image_content_type(&bytes))
                .header(header::CACHE_CONTROL, "max-age=259200")
                .body(Body::from(bytes))
                .unwrap_or_default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_image_types() {
        assert_eq!(image_content_type(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(image_content_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(image_content_type(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(image_content_type(b"xyz"), "application/octet-stream");
    }
}
