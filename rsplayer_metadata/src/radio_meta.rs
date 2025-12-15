use std::io::{Read, Result as IoResult};

use api_models::player::Song;
use api_models::state::StateChangeEvent;
use log::info;
use serde_json;

use tokio::sync::broadcast::Sender;
use ureq;

#[derive(Clone, Debug)]
pub struct RadioMeta {
    pub name: Option<String>,
    pub description: Option<String>,
    pub url: String,
    pub genre: Option<String>,
    pub image_url: Option<String>,
    pub samplerate: Option<u32>,
    pub channels: Option<usize>,
    pub bitrate: Option<u32>,
}

pub struct IcyMetadataReader<R: Read> {
    inner: R,
    metaint: usize,
    remaining: usize,
    changes_tx: Sender<StateChangeEvent>,
    last_title: String,
    radio_meta: RadioMeta,
}

impl<R: Read> IcyMetadataReader<R> {
    pub const fn new(inner: R, metaint: usize, changes_tx: Sender<StateChangeEvent>, radio_meta: RadioMeta) -> Self {
        Self {
            inner,
            metaint,
            remaining: metaint,
            changes_tx,
            last_title: String::new(),
            radio_meta,
        }
    }

    fn parse_metadata(&mut self) -> IoResult<()> {
        let mut len_byte = [0u8];
        self.inner.read_exact(&mut len_byte)?;
        let len = len_byte[0] as usize * 16;

        if len > 0 {
            let mut metadata_buf = vec![0u8; len];
            self.inner.read_exact(&mut metadata_buf)?;

            if let Ok(metadata_str) = std::str::from_utf8(&metadata_buf) {
                info!("metadata:{metadata_str}");
                if let Some(title_part) = metadata_str.split("StreamTitle='").nth(1) {
                    if let Some(title) = title_part.split("';").next() {
                        if !title.is_empty() && title != self.last_title {
                            self.last_title = title.to_string();
                            let parts: Vec<&str> = title.splitn(2, " - ").collect();
                            let (artist, song_title) = if parts.len() == 2 {
                                (Some(parts[0].to_string()), Some(parts[1].to_string()))
                            } else {
                                (None, Some(title.to_string()))
                            };
                            let mut album = self.radio_meta.description.clone().unwrap_or_default();
                            if album.is_empty(){
                                album = self.radio_meta.name.clone().unwrap_or_default();
                            }
                            if album.is_empty(){
                                album = self.radio_meta.url.clone();
                            }
                            let song = Song {
                                title: song_title,
                                artist,
                                album: Some(album),
                                genre: self.radio_meta.genre.clone(),
                                file: self.radio_meta.url.clone(),
                                image_url: self.radio_meta.image_url.clone(),
                                ..Default::default()
                            };
                            self.changes_tx.send(StateChangeEvent::CurrentSongEvent(song)).ok();
                        }
                    }
                }
            }
        }
        self.remaining = self.metaint;
        Ok(())
    }
}

impl<R: Read> Read for IcyMetadataReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.remaining == 0 {
            if let Err(e) = self.parse_metadata() {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(0);
                }
                return Err(e);
            }
        }

        let read_len = std::cmp::min(buf.len(), self.remaining);
        let bytes_read = self.inner.read(&mut buf[..read_len])?;

        if bytes_read == 0 {
            return Ok(0); // EOF
        }

        self.remaining -= bytes_read;
        Ok(bytes_read)
    }
}

pub fn get_external_radio_meta(
    agent: &ureq::Agent,
    resp: &ureq::Response,
) -> Option<RadioMeta> {
    let final_url = resp.get_url();
    let mut radio_meta = RadioMeta {
        name: resp.header("icy-name").map(ToString::to_string),
        description: resp.header("icy-description").map(ToString::to_string),
        genre: resp.header("icy-genre").map(ToString::to_string),
        url: resp.header("icy-url").map_or_else(|| final_url.to_string(), ToString::to_string),
        image_url: None,
        samplerate: None,
        channels: None,
        bitrate: None,
    };

    if let Some(audio_info) = resp.header("ice-audio-info") {
        audio_info.split(';').for_each(|s| {
            if let Some((key, value)) = s.split_once('=') {
                match key.trim() {
                    "samplerate" => {
                        if let Ok(val) = value.trim().parse() {
                            radio_meta.samplerate = Some(val);
                        }
                    }
                    "channels" => {
                        if let Ok(val) = value.trim().parse() {
                            radio_meta.channels = Some(val);
                        }
                    }
                    "bitrate" => {
                        if let Ok(val) = value.trim().parse() {
                            radio_meta.bitrate = Some(val);
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    let server = resp.header("Server").unwrap_or_default();
    if server == "radiosphere" {
        let track_url = build_radiosphere_api_url(final_url);

        if let Some(url) = track_url {
            if let Ok(api_resp) = agent.get(&url).call() {
                if let Ok(body) = api_resp.into_string() {
                    if let Some(song) = parse_song_metadata(&body) {
                        radio_meta.name = Some(format!(
                            "{} - {}",
                            song.artist.unwrap_or_default(),
                            song.title.unwrap_or_default()
                        ));
                    }
                }
            }
        }
        if let Some((channel_url, source)) = build_radiosphere_channel_api_url(final_url) {
            if let Ok(api_resp) = agent.get(&channel_url).call() {
                if let Ok(api_body) = api_resp.into_string() {
                    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&api_body) {
                        if let Some(cover_image_url) = json_body.get("coverImageUrl").and_then(|v| v.as_str()) {
                            radio_meta.image_url = Some(cover_image_url.to_string());
                        }
                        let channel = json_body.get("title").and_then(|v| v.as_str());
                        let station = source.split('.').next().map(|s| {
                            let mut c = s.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
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
            }
        }
    } else if server.starts_with("QuantumCast Streamer") {
        if let Some(channel_key) = resp.header("x-quantumcast-channelkey") {
            let track_url = Some(format!(
                "https://api.streamabc.net/metadata/channel/{channel_key}.json"
            ));
            
            if let Some(url) = track_url {
                if let Ok(api_resp) = agent.get(&url).call() {
                    if let Ok(body) = api_resp.into_string() {
                        if let Some(song) = parse_song_metadata(&body) {
                            radio_meta.name = Some(format!(
                                "{} - {}",
                                song.artist.unwrap_or_default(),
                                song.title.unwrap_or_default()
                            ));
                        }
                        if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(&body) {
                            if let Some(cover_url) = json_body.get("cover").and_then(|v| v.as_str()) {
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
            }
        }
    }
    Some(radio_meta)
}

fn build_radiosphere_api_url(final_url: &str) -> Option<String> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?;
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string
        .split('&')
        .find(|p| p.starts_with("source="))?;
    let source = source_param.split('=').nth(1)?;
    Some(format!(
        "https://{source}/channels/{channel_id}/current-track"
    ))
}

fn parse_song_metadata(api_body: &str) -> Option<Song> {
    let json_body = serde_json::from_str::<serde_json::Value>(api_body).ok()?;

    let title: Option<&str>;
    let artist: Option<&str>;

    if let Some(track_info) = json_body.get("trackInfo") {
        // Radiosphere
        title = track_info.get("title").and_then(|v| v.as_str());
        artist = track_info.get("artistCredits").and_then(|v| v.as_str());
    } else {
        // QuantumCast
        title = json_body.get("song").and_then(|v| v.as_str());
        artist = json_body.get("artist").and_then(|v| v.as_str());
    }

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

fn build_radiosphere_channel_api_url(final_url: &str) -> Option<(String, String)> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?.to_string();
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string
        .split('&')
        .find(|p| p.starts_with("source="))?;
    let source = source_param.split('=').nth(1)?.to_string();
    Some((
        format!("https://{source}/channels/{channel_id}/"),
        source,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quantumcast_metadata() {
        let json_body = r#"{"artist": "The Beatles", "song": "Strawberry Fields Forever"}"#;
        let song = parse_song_metadata(json_body).unwrap();
        assert_eq!(song.artist, Some("The Beatles".to_string()));
        assert_eq!(song.title, Some("Strawberry Fields Forever".to_string()));
    }

    #[test]
    fn test_parse_radiosphere_metadata() {
        let json_body = r#"{
          "trackInfo": {
            "title": "SH-101 Dalmatians (Audio Soul Project Version)",
            "artistCredits": "Manik (NYC)"
          }
        }"#;
        let song = parse_song_metadata(json_body).unwrap();
        assert_eq!(song.artist, Some("Manik (NYC)".to_string()));
        assert_eq!(
            song.title,
            Some("SH-101 Dalmatians (Audio Soul Project Version)".to_string())
        );
    }
}