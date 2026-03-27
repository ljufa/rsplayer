use api_models::player::Song;
use serde_json;

use crate::radio_meta::RadioMeta;

pub fn process_radiosphere_meta(agent: &ureq::Agent, final_url: &str, radio_meta: &mut RadioMeta) {
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
        }
    }
}

pub fn process_quantumcast_meta(agent: &ureq::Agent, channel_key: &str, radio_meta: &mut RadioMeta) {
    let track_url = format!("https://api.streamabc.net/metadata/channel/{channel_key}.json");

    if let Ok(api_resp) = agent.get(&track_url).call() {
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

fn build_radiosphere_api_url(final_url: &str) -> Option<String> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?;
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string.split('&').find(|p| p.starts_with("source="))?;
    let source = source_param.split('=').nth(1)?;
    Some(format!("https://{source}/channels/{channel_id}/current-track"))
}

fn build_radiosphere_channel_api_url(final_url: &str) -> Option<(String, String)> {
    let channel_id_part = final_url.split("/channels/").nth(1)?;
    let channel_id = channel_id_part.split('/').next()?.to_string();
    let query_string = final_url.split('?').nth(1)?;
    let source_param = query_string.split('&').find(|p| p.starts_with("source="))?;
    let source = source_param.split('=').nth(1)?.to_string();
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
