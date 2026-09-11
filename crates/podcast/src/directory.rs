//! Podcast directory search: iTunes Search API (no credentials) and
//! Podcast Index (API key + secret from settings).
//!
//! Both run on the podcast worker thread with a short-lived agent. Responses
//! are mapped to [`PodcastSearchResult`]; hits without a feed URL are dropped
//! because nothing can be subscribed without one.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use log::debug;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use ureq::Agent;

use api_models::podcast::{PodcastDirectory, PodcastSearchResult};
use api_models::settings::PodcastSettings;

const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";
const PODCAST_INDEX_SEARCH_URL: &str = "https://api.podcastindex.org/api/1.0/search/byterm";
const MAX_RESULTS: usize = 50;
const RESPONSE_LIMIT_BYTES: u64 = 8 * 1024 * 1024;

pub trait PodcastDirectorySearch: Send + Sync {
    fn search(&self, term: &str) -> Result<Vec<PodcastSearchResult>>;
}

/// Picks the directory from settings. Podcast Index without credentials
/// falls back to iTunes; the returned flag says so, for a user notification.
pub fn directory_for(settings: &PodcastSettings) -> (Box<dyn PodcastDirectorySearch>, bool) {
    match settings.directory {
        PodcastDirectory::PodcastIndex
            if !settings.podcast_index_api_key.trim().is_empty() && !settings.podcast_index_api_secret.trim().is_empty() =>
        {
            (
                Box::new(PodcastIndexDirectory {
                    api_key: settings.podcast_index_api_key.trim().to_string(),
                    api_secret: settings.podcast_index_api_secret.trim().to_string(),
                }),
                false,
            )
        }
        PodcastDirectory::PodcastIndex => (Box::new(ItunesDirectory), true),
        PodcastDirectory::Itunes => (Box::new(ItunesDirectory), false),
    }
}

pub fn directory_agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn user_agent() -> String {
    format!("rsplayer/{}", env!("CARGO_PKG_VERSION"))
}

fn read_json<T: serde::de::DeserializeOwned>(resp: ureq::http::Response<ureq::Body>, what: &str) -> Result<T> {
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(anyhow!("{what} answered HTTP {status}"));
    }
    let bytes = resp
        .into_body()
        .into_with_config()
        .limit(RESPONSE_LIMIT_BYTES)
        .read_to_vec()
        .with_context(|| format!("{what}: read body"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("{what}: decode response"))
}

// ---------------------------------------------------------------- iTunes

pub struct ItunesDirectory;

#[derive(Debug, Deserialize)]
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesResult {
    #[serde(default)]
    collection_name: Option<String>,
    #[serde(default)]
    artist_name: Option<String>,
    #[serde(default)]
    feed_url: Option<String>,
    #[serde(default)]
    artwork_url600: Option<String>,
    #[serde(default)]
    artwork_url100: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    track_count: Option<u32>,
}

impl ItunesDirectory {
    fn map(resp: ItunesResponse) -> Vec<PodcastSearchResult> {
        resp.results
            .into_iter()
            .filter_map(|r| {
                let feed_url = r.feed_url.filter(|u| u.starts_with("http"))?;
                Some(PodcastSearchResult {
                    title: r.collection_name.unwrap_or_else(|| feed_url.clone()),
                    author: r.artist_name.filter(|a| !a.is_empty()),
                    feed_url,
                    image_url: r.artwork_url600.or(r.artwork_url100),
                    description: None,
                    categories: r.genres.into_iter().filter(|g| g != "Podcasts").collect(),
                    episode_count: r.track_count,
                    source: PodcastDirectory::Itunes,
                })
            })
            .collect()
    }
}

impl PodcastDirectorySearch for ItunesDirectory {
    fn search(&self, term: &str) -> Result<Vec<PodcastSearchResult>> {
        let resp = directory_agent()
            .get(ITUNES_SEARCH_URL)
            .query("media", "podcast")
            .query("entity", "podcast")
            .query("limit", MAX_RESULTS.to_string())
            .query("term", term)
            .call()
            .context("iTunes search request")?;
        let parsed: ItunesResponse = read_json(resp, "iTunes search")?;
        let results = Self::map(parsed);
        debug!("iTunes search '{term}': {} results", results.len());
        Ok(results)
    }
}

// ---------------------------------------------------------- Podcast Index

pub struct PodcastIndexDirectory {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Deserialize)]
struct PodcastIndexResponse {
    #[serde(default)]
    feeds: Vec<PodcastIndexFeed>,
}

#[derive(Debug, Deserialize)]
struct PodcastIndexFeed {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    artwork: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    categories: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    episode_count: Option<u32>,
}

impl PodcastIndexDirectory {
    /// `Authorization` = hex SHA-1 of `key + secret + unix-time`, per the
    /// Podcast Index API docs.
    fn auth_headers(&self, unix_time: u64) -> [(String, String); 3] {
        let mut hasher = Sha1::new();
        hasher.update(self.api_key.as_bytes());
        hasher.update(self.api_secret.as_bytes());
        hasher.update(unix_time.to_string().as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        [
            ("X-Auth-Key".to_string(), self.api_key.clone()),
            ("X-Auth-Date".to_string(), unix_time.to_string()),
            ("Authorization".to_string(), hash),
        ]
    }

    fn map(resp: PodcastIndexResponse) -> Vec<PodcastSearchResult> {
        resp.feeds
            .into_iter()
            .filter_map(|f| {
                let feed_url = f.url.filter(|u| u.starts_with("http"))?;
                Some(PodcastSearchResult {
                    title: f.title.filter(|t| !t.is_empty()).unwrap_or_else(|| feed_url.clone()),
                    author: f.author.filter(|a| !a.is_empty()),
                    feed_url,
                    image_url: f.artwork.filter(|a| !a.is_empty()).or(f.image).filter(|a| !a.is_empty()),
                    description: f
                        .description
                        .map(|d| crate::feed::strip_html(&d, crate::feed::MAX_DESCRIPTION_CHARS))
                        .filter(|d| !d.is_empty()),
                    categories: f.categories.map(|c| c.into_values().collect()).unwrap_or_default(),
                    episode_count: f.episode_count,
                    source: PodcastDirectory::PodcastIndex,
                })
            })
            .collect()
    }
}

impl PodcastDirectorySearch for PodcastIndexDirectory {
    fn search(&self, term: &str) -> Result<Vec<PodcastSearchResult>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut req = directory_agent()
            .get(PODCAST_INDEX_SEARCH_URL)
            .query("q", term)
            .query("max", MAX_RESULTS.to_string());
        for (name, value) in self.auth_headers(now) {
            req = req.header(name, value);
        }
        let resp = req.call().context("Podcast Index search request")?;
        let parsed: PodcastIndexResponse = read_json(resp, "Podcast Index search")?;
        let results = Self::map(parsed);
        debug!("Podcast Index search '{term}': {} results", results.len());
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn itunes_mapping_drops_hits_without_feed_and_prefers_large_artwork() {
        let json = r#"{"resultCount":3,"results":[
          {"collectionName":"Show A","artistName":"Host A","feedUrl":"https://a/feed","artworkUrl100":"https://a/100.jpg","artworkUrl600":"https://a/600.jpg","genres":["Podcasts","Technology"],"trackCount":120},
          {"collectionName":"No feed","artistName":"X"},
          {"collectionName":"Show B","feedUrl":"https://b/feed","artworkUrl100":"https://b/100.jpg","genres":[]}
        ]}"#;
        let parsed: ItunesResponse = serde_json::from_str(json).unwrap();
        let results = ItunesDirectory::map(parsed);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Show A");
        assert_eq!(results[0].image_url.as_deref(), Some("https://a/600.jpg"));
        assert_eq!(results[0].categories, vec!["Technology"]);
        assert_eq!(results[0].episode_count, Some(120));
        assert_eq!(results[0].source, PodcastDirectory::Itunes);
        assert_eq!(results[1].image_url.as_deref(), Some("https://b/100.jpg"));
        assert_eq!(results[1].author, None);
    }

    #[test]
    fn podcast_index_mapping_and_auth() {
        let json = r#"{"status":"true","feeds":[
          {"id":1,"title":"PI Show","url":"https://pi/feed","author":"Someone","artwork":"https://pi/art.jpg","image":"https://pi/img.jpg","description":"<p>Desc</p>","categories":{"55":"News","59":"Politics"},"episodeCount":10},
          {"id":2,"title":"Broken","url":""}
        ]}"#;
        let parsed: PodcastIndexResponse = serde_json::from_str(json).unwrap();
        let results = PodcastIndexDirectory::map(parsed);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description.as_deref(), Some("Desc"));
        assert_eq!(results[0].categories, vec!["News", "Politics"]);
        assert_eq!(results[0].image_url.as_deref(), Some("https://pi/art.jpg"));

        let dir = PodcastIndexDirectory {
            api_key: "key".into(),
            api_secret: "secret".into(),
        };
        let headers = dir.auth_headers(1_700_000_000);
        assert_eq!(headers[0].1, "key");
        assert_eq!(headers[1].1, "1700000000");
        // sha1("keysecret1700000000")
        assert_eq!(headers[2].1, stable_sha1("keysecret1700000000"));
    }

    fn stable_sha1(s: &str) -> String {
        let mut h = Sha1::new();
        h.update(s.as_bytes());
        format!("{:x}", h.finalize())
    }

    #[test]
    fn directory_selection_falls_back_without_credentials() {
        let mut settings = PodcastSettings::default();
        assert!(!directory_for(&settings).1);
        settings.directory = PodcastDirectory::PodcastIndex;
        assert!(directory_for(&settings).1, "missing credentials → fallback flagged");
        settings.podcast_index_api_key = "k".into();
        settings.podcast_index_api_secret = "s".into();
        assert!(!directory_for(&settings).1);
    }
}
