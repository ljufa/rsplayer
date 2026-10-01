//! Station-level radio metadata from HTTP response headers (`icy-name`,
//! `ice-audio-info`…), enriched by provider-specific lookups in
//! [`radio_providers`].

use api_models::player::Song;

use crate::radio_providers;

/// The track a station's metadata service reports as playing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NowPlaying {
    pub artist: Option<String>,
    pub title: String,
    pub image_url: Option<String>,
}

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
    /// Current track from the station's metadata service (QuantumCast, Radiosphere).
    pub now_playing: Option<NowPlaying>,
    /// That service's now-playing URL. When set, it is polled during playback
    /// and the stream's own ICY titles (often just the channel name) are ignored.
    pub now_playing_url: Option<String>,
}

impl RadioMeta {
    /// The song to publish for a track playing on this station.
    pub fn track_song(&self, artist: Option<String>, title: String, image_url: Option<String>) -> Song {
        let album = [&self.description, &self.name]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| self.url.clone());
        Song {
            title: Some(title),
            artist,
            album: Some(album),
            genre: self.genre.clone(),
            file: self.url.clone(),
            image_url: image_url.or_else(|| self.image_url.clone()),
            ..Default::default()
        }
    }
}

pub fn get_external_radio_meta(agent: &ureq::Agent, resp: &ureq::http::Response<ureq::Body>) -> Option<RadioMeta> {
    use ureq::ResponseExt;

    let final_url = resp.get_uri().to_string();
    let header_str = |name: &str| -> Option<String> { resp.headers().get(name).and_then(|v| v.to_str().ok()).map(ToString::to_string) };

    let mut radio_meta = RadioMeta {
        name: header_str("icy-name"),
        description: header_str("icy-description"),
        genre: header_str("icy-genre"),
        url: header_str("icy-url").unwrap_or_else(|| final_url.clone()),
        image_url: None,
        samplerate: None,
        channels: None,
        bitrate: None,
        now_playing: None,
        now_playing_url: None,
    };

    if let Some(audio_info) = header_str("ice-audio-info") {
        parse_audio_info(&audio_info, &mut radio_meta);
    }

    let server = resp.headers().get("Server").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if server == "radiosphere" {
        radio_providers::process_radiosphere_meta(agent, &final_url, &mut radio_meta);
    } else if server.starts_with("QuantumCast Streamer")
        && let Some(channel_key) = resp.headers().get("x-quantumcast-channelkey").and_then(|v| v.to_str().ok())
    {
        radio_providers::process_quantumcast_meta(agent, channel_key, &mut radio_meta);
    }
    Some(radio_meta)
}

fn parse_audio_info(audio_info: &str, radio_meta: &mut RadioMeta) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quantumcast_metadata() {
        let json_body = r#"{"artist": "The Beatles", "song": "Strawberry Fields Forever"}"#;
        let song = radio_providers::parse_song_metadata(json_body).unwrap();
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
        let song = radio_providers::parse_song_metadata(json_body).unwrap();
        assert_eq!(song.artist, Some("Manik (NYC)".to_string()));
        assert_eq!(song.title, Some("SH-101 Dalmatians (Audio Soul Project Version)".to_string()));
    }
}
