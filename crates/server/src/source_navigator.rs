//! Radio and podcast playback sources for the player.
//!
//! [`AppSourceNavigator`] turns a [`PlaybackSource`] into the `Song` the
//! player decodes and decides what Next/Prev (and the end of an episode)
//! move to: radio cycles through the favorite and custom stations, podcasts
//! walk the show's episodes by publish date.

use std::sync::Arc;

use api_models::playback_source::PlaybackSource;
use api_models::player::Song;
use api_models::radio::RadioStation;
use metadata::metadata_service::MetadataService;
use playback::rsp::player_service::SourceNavigator;
use podcast::PodcastService;

pub struct AppSourceNavigator {
    pub metadata_service: Arc<MetadataService>,
    pub podcast_service: Arc<PodcastService>,
}

impl SourceNavigator for AppSourceNavigator {
    fn song_for(&self, source: &PlaybackSource) -> Option<Song> {
        match source {
            PlaybackSource::Queue => None,
            PlaybackSource::Radio(station) => Some(station_song(station)),
            PlaybackSource::Podcast { episode_id, .. } => self
                .podcast_service
                .episode_with_podcast(episode_id)
                .map(|(episode, podcast)| episode.to_song(&podcast)),
        }
    }

    fn next(&self, source: &PlaybackSource, finished: bool) -> Option<PlaybackSource> {
        match source {
            PlaybackSource::Queue => None,
            PlaybackSource::Radio(station) => {
                adjacent_station(&self.metadata_service.get_zap_stations(), &station.url, true).map(PlaybackSource::Radio)
            }
            PlaybackSource::Podcast { episode_id, .. } => {
                let svc = &self.podcast_service;
                // Auto-advance only continues with something not heard yet;
                // a listener's Next may also land on a played episode.
                let next = svc
                    .adjacent_episode(episode_id, true, true)
                    .or_else(|| if finished { None } else { svc.adjacent_episode(episode_id, true, false) })?;
                Some(episode_source(next.podcast_id, next.id))
            }
        }
    }

    fn prev(&self, source: &PlaybackSource) -> Option<PlaybackSource> {
        match source {
            PlaybackSource::Queue => None,
            PlaybackSource::Radio(station) => {
                adjacent_station(&self.metadata_service.get_zap_stations(), &station.url, false).map(PlaybackSource::Radio)
            }
            PlaybackSource::Podcast { episode_id, .. } => {
                let prev = self.podcast_service.adjacent_episode(episode_id, false, false)?;
                Some(episode_source(prev.podcast_id, prev.id))
            }
        }
    }
}

const fn episode_source(podcast_id: String, episode_id: String) -> PlaybackSource {
    PlaybackSource::Podcast { podcast_id, episode_id }
}

fn station_song(station: &RadioStation) -> Song {
    Song {
        file: station.url.clone(),
        title: Some(station.name.clone()),
        image_url: station.image_url.clone(),
        ..Default::default()
    }
}

/// The station after (`forward`) or before the one streaming `url`, wrapping
/// around. A station that is not in the list (played from search results)
/// moves to the first or last one.
fn adjacent_station(stations: &[RadioStation], url: &str, forward: bool) -> Option<RadioStation> {
    let len = stations.len();
    if len == 0 {
        return None;
    }
    let target = match stations.iter().position(|s| s.url == url) {
        Some(pos) if forward => (pos + 1) % len,
        Some(pos) => (pos + len - 1) % len,
        None if forward => 0,
        None => len - 1,
    };
    stations.get(target).filter(|s| s.url != url).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str) -> RadioStation {
        RadioStation {
            name: name.to_string(),
            url: format!("http://{name}/stream"),
            ..Default::default()
        }
    }

    #[test]
    fn adjacent_station_wraps_around() {
        let list = [station("a"), station("b"), station("c")];
        let name = |s: Option<RadioStation>| s.map(|s| s.name);
        assert_eq!(name(adjacent_station(&list, "http://a/stream", true)), Some("b".into()));
        assert_eq!(name(adjacent_station(&list, "http://c/stream", true)), Some("a".into()));
        assert_eq!(name(adjacent_station(&list, "http://a/stream", false)), Some("c".into()));
        assert_eq!(name(adjacent_station(&list, "http://other/stream", true)), Some("a".into()));
        assert_eq!(name(adjacent_station(&list, "http://other/stream", false)), Some("c".into()));
    }

    #[test]
    fn adjacent_station_has_nowhere_to_go_alone() {
        assert_eq!(adjacent_station(&[], "http://a/stream", true), None);
        assert_eq!(adjacent_station(&[station("a")], "http://a/stream", true), None);
    }
}
