use crate::radio_providers;

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

pub fn get_external_radio_meta(agent: &ureq::Agent, resp: &ureq::Response) -> Option<RadioMeta> {
    let final_url = resp.get_url();
    let mut radio_meta = RadioMeta {
        name: resp.header("icy-name").map(ToString::to_string),
        description: resp.header("icy-description").map(ToString::to_string),
        genre: resp.header("icy-genre").map(ToString::to_string),
        url: resp
            .header("icy-url")
            .map_or_else(|| final_url.to_string(), ToString::to_string),
        image_url: None,
        samplerate: None,
        channels: None,
        bitrate: None,
    };

    if let Some(audio_info) = resp.header("ice-audio-info") {
        parse_audio_info(audio_info, &mut radio_meta);
    }

    let server = resp.header("Server").unwrap_or_default();
    if server == "radiosphere" {
        radio_providers::process_radiosphere_meta(agent, final_url, &mut radio_meta);
    } else if server.starts_with("QuantumCast Streamer") {
        if let Some(channel_key) = resp.header("x-quantumcast-channelkey") {
            radio_providers::process_quantumcast_meta(agent, channel_key, &mut radio_meta);
        }
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
        assert_eq!(
            song.title,
            Some("SH-101 Dalmatians (Audio Soul Project Version)".to_string())
        );
    }
}
