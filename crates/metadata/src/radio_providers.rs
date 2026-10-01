//! Per-provider API lookups for richer radio metadata (Radiosphere and
//! QuantumCast: now-playing track and channel artwork), keyed off the stream
//! response. [`NowPlayingPoller`] keeps asking the provider during playback,
//! since its ICY titles often only name the channel.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use api_models::player::Song;
use api_models::state::StateChangeEvent;
use log::debug;
use serde_json;
use tokio::sync::broadcast::Sender;

use crate::radio_meta::{NowPlaying, RadioMeta};

/// How often the provider is asked for the current track.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub fn process_radiosphere_meta(agent: &ureq::Agent, final_url: &str, radio_meta: &mut RadioMeta) {
    if let Some(url) = build_radiosphere_api_url(final_url)
        && let Some(body) = fetch_body(agent, &url)
    {
        radio_meta.now_playing = parse_now_playing(&body);
        radio_meta.now_playing_url = Some(url);
    }

    if let Some((channel_url, source)) = build_radiosphere_channel_api_url(final_url)
        && let Ok(api_resp) = agent.get(&channel_url).call()
        && let Ok(api_body) = api_resp.into_body().read_to_string()
        && let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&api_body)
    {
        if let Some(cover_image_url) = json_body.get("coverImageUrl").and_then(|v| v.as_str()).filter(|u| !u.trim().is_empty()) {
            radio_meta.image_url = Some(cover_image_url.to_string());
        }
        let channel = json_body.get("title").and_then(|v| v.as_str());
        let station = source.split('.').next().map(|s| {
            let mut c = s.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
        });
        let mut new_description = String::new();
        if let Some(station_str) = station {
            new_description.push_str(&station_str);
        }
        if let Some(channel_str) = channel {
            if !new_description.is_empty() {
                new_description.push_str(" - ");
            }
            new_description.push_str(channel_str);
        }
        if !new_description.is_empty() {
            radio_meta.description = Some(new_description);
        }
    }
}

pub fn process_quantumcast_meta(agent: &ureq::Agent, channel_key: &str, radio_meta: &mut RadioMeta) {
    let track_url = format!("https://api.streamabc.net/metadata/channel/{channel_key}.json");

    if let Some(body) = fetch_body(agent, &track_url) {
        radio_meta.now_playing = parse_now_playing(&body);
        // Only poll a service that answered.
        radio_meta.now_playing_url = Some(track_url);
        if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&body) {
            // QuantumCast sends `"cover":""` when it has no artwork.
            if let Some(cover_url) = json_body.get("cover").and_then(|v| v.as_str()).filter(|u| !u.trim().is_empty()) {
                radio_meta.image_url = Some(cover_url.to_string());
            }
            let channel = json_body.get("channel").and_then(|v| v.as_str());
            let station = json_body.get("station").and_then(|v| v.as_str());
            let mut new_description = String::new();
            if let Some(station_str) = station {
                new_description.push_str(station_str);
            }
            if let Some(channel_str) = channel {
                if !new_description.is_empty() {
                    new_description.push_str(" - ");
                }
                new_description.push_str(channel_str);
            }

            if !new_description.is_empty() {
                radio_meta.description = Some(new_description);
            }
        }
    }
}

fn build_radiosphere_api_url(final_url: &str) -> Option<String> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?;
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string.split('&').find(|p| p.starts_with("source="))?;
    let source = source_param.split('=').nth(1).filter(|s| s.contains('.'))?;
    Some(format!("https://{source}/channels/{channel_id}/current-track"))
}

fn build_radiosphere_channel_api_url(final_url: &str) -> Option<(String, String)> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?.to_string();
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string.split('&').find(|p| p.starts_with("source="))?;
    // `source` is the API host only on some streams (`source=externalembedflxhp` is not).
    let source = source_param.split('=').nth(1).filter(|s| s.contains('.'))?.to_string();
    Some((format!("https://{source}/channels/{channel_id}/"), source))
}

pub fn parse_song_metadata(api_body: &str) -> Option<Song> {
    let json_body = serde_json::from_str::<serde_json::Value>(api_body).ok()?;

    let (title, artist) = json_body.get("trackInfo").map_or_else(
        || {
            (
                json_body.get("song").and_then(|v| v.as_str()),
                json_body.get("artist").and_then(|v| v.as_str()),
            )
        },
        |track_info| {
            (
                track_info.get("title").and_then(|v| v.as_str()),
                track_info.get("artistCredits").and_then(|v| v.as_str()),
            )
        },
    );

    if title.is_some() || artist.is_some() {
        Some(Song {
            title: title.map(ToString::to_string),
            artist: artist.map(ToString::to_string),
            ..Default::default()
        })
    } else {
        None
    }
}

/// The current track from a provider response, or `None` when it names no
/// track (empty fields during ads and jingles).
pub fn parse_now_playing(api_body: &str) -> Option<NowPlaying> {
    let song = parse_song_metadata(api_body)?;
    let non_empty = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let title = non_empty(song.title)?;
    // Per-track artwork: QuantumCast `cover` (empty when it has none),
    // Radiosphere `trackInfo.artwork`.
    let json = serde_json::from_str::<serde_json::Value>(api_body).ok()?;
    let image_url = [json.get("cover"), json.get("trackInfo").and_then(|t| t.get("artwork"))]
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .find(|u| u.starts_with("http://") || u.starts_with("https://"))
        .map(str::to_string);
    Some(NowPlaying {
        artist: non_empty(song.artist),
        title,
        image_url,
    })
}

fn fetch_body(agent: &ureq::Agent, url: &str) -> Option<String> {
    agent.get(url).call().ok()?.into_body().read_to_string().ok()
}

fn fetch_now_playing(agent: &ureq::Agent, url: &str) -> Option<NowPlaying> {
    parse_now_playing(&fetch_body(agent, url)?)
}

/// Polls the station's now-playing URL in a background thread and publishes
/// each track change. Stops when dropped.
pub struct NowPlayingPoller {
    stop: Arc<AtomicBool>,
}

impl NowPlayingPoller {
    /// Starts polling, or returns `None` when the station has no metadata service.
    pub fn start(radio_meta: RadioMeta, changes_tx: Sender<StateChangeEvent>) -> Option<Self> {
        let url = radio_meta.now_playing_url.clone()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let spawned = std::thread::Builder::new().name("radio-now-playing".into()).spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build()
                .into();
            // The first track was already published with the station.
            let mut last = radio_meta.now_playing.clone();
            loop {
                let mut waited = Duration::ZERO;
                while waited < POLL_INTERVAL {
                    if stop_thread.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                    waited += Duration::from_millis(250);
                }
                let Some(now) = fetch_now_playing(&agent, &url) else {
                    debug!("No track from {url}");
                    continue;
                };
                if last.as_ref() != Some(&now) && !stop_thread.load(Ordering::Relaxed) {
                    let song = radio_meta.track_song(now.artist.clone(), now.title.clone(), now.image_url.clone());
                    changes_tx.send(StateChangeEvent::CurrentSongEvent(song)).ok();
                    last = Some(now);
                }
            }
        });
        spawned.ok().map(|_| Self { stop })
    }
}

impl Drop for NowPlayingPoller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A stream body that keeps its [`NowPlayingPoller`] alive as long as the
/// stream is being read.
pub struct PolledReader<R> {
    inner: R,
    _poller: Option<NowPlayingPoller>,
}

impl<R: Read> PolledReader<R> {
    pub const fn new(inner: R, poller: Option<NowPlayingPoller>) -> Self {
        Self { inner, _poller: poller }
    }
}

impl<R: Read> Read for PolledReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_playing_from_quantumcast() {
        let body = r#"{"channel":"FluxFM - Livestream","artist":"Mac Miller","song":"Butterflies","station":"fluxfm","cover":"","album":""}"#;
        assert_eq!(
            parse_now_playing(body),
            Some(NowPlaying {
                artist: Some("Mac Miller".into()),
                title: "Butterflies".into(),
                image_url: None,
            })
        );
    }

    #[test]
    fn now_playing_keeps_cover() {
        let body = r#"{"artist":"A","song":"B","cover":"https://img.example/b.jpg"}"#;
        assert_eq!(parse_now_playing(body).and_then(|n| n.image_url).as_deref(), Some("https://img.example/b.jpg"));
    }

    #[test]
    fn now_playing_from_radiosphere() {
        let body = r#"{"trackInfo":{"title":"You Make Me Wanna...","artistCredits":"Usher","artwork":"https://fluxmusic.cdn.radiosphere.io/images/0b31969c"}}"#;
        assert_eq!(
            parse_now_playing(body),
            Some(NowPlaying {
                artist: Some("Usher".into()),
                title: "You Make Me Wanna...".into(),
                image_url: Some("https://fluxmusic.cdn.radiosphere.io/images/0b31969c".into()),
            })
        );
    }

    #[test]
    fn radiosphere_url_needs_api_host() {
        let base = "http://edge.streams.radiosphere.io/557b/channels/7ee2/stream.aac";
        assert_eq!(
            build_radiosphere_api_url(&format!("{base}?source=fluxmusic.api.radiosphere.io")).as_deref(),
            Some("https://fluxmusic.api.radiosphere.io/channels/7ee2/current-track")
        );
        assert_eq!(build_radiosphere_api_url(&format!("{base}?source=externalembedflxhp")), None);
    }

    #[test]
    fn no_now_playing_without_title() {
        assert_eq!(parse_now_playing(r#"{"artist":"","song":""}"#), None);
        assert_eq!(parse_now_playing(r#"{"artist":"Jingle","song":" "}"#), None);
        assert_eq!(parse_now_playing("not json"), None);
    }
}
