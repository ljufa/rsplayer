//! RSS/Atom feed → [`ParsedFeed`], via `feed-rs`.
//!
//! Only what the UI and player need is kept: show-level metadata, and per
//! item the enclosure (first `audio/*` media content, else the first with a
//! URL), `itunes:duration`, artwork and a plain-text description capped at
//! [`MAX_DESCRIPTION_CHARS`] characters.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use feed_rs::model::{Entry, Feed, Text};
use sha1::{Digest, Sha1};

use api_models::podcast::Episode;

pub const MAX_DESCRIPTION_CHARS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFeed {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub website: Option<String>,
    pub categories: Vec<String>,
    pub episodes: Vec<Episode>,
}

/// Hex SHA-1 of `input`; ids for podcasts (feed URL) and episodes.
#[must_use]
pub fn stable_id(input: &str) -> String {
    sha1_hex(&[input.as_bytes()])
}

/// Lowercase hex SHA-1 of the concatenated `parts`. Stored podcast and
/// episode ids depend on this exact format.
pub(crate) fn sha1_hex(parts: &[&[u8]]) -> String {
    use std::fmt::Write;

    let mut hasher = Sha1::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().iter().fold(String::with_capacity(40), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// Parses feed bytes; `podcast_id` seeds the episode ids. Items without an
/// audio enclosure are skipped.
pub fn parse_feed(bytes: &[u8], podcast_id: &str) -> Result<ParsedFeed> {
    let feed: Feed = feed_rs::parser::parse(bytes).context("feed parse failed")?;
    let title = feed
        .title
        .as_ref()
        .map(|t| plain_text(t, 200))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled podcast".to_string());
    let author = feed
        .authors
        .iter()
        .chain(feed.contributors.iter())
        .map(|p| p.name.trim().to_string())
        .find(|n| !n.is_empty());
    let description = feed.description.as_ref().map(|t| plain_text(t, MAX_DESCRIPTION_CHARS)).filter(|s| !s.is_empty());
    let image_url = feed
        .logo
        .as_ref()
        .or(feed.icon.as_ref())
        .map(|i| i.uri.clone())
        .filter(|u| !u.is_empty());
    let website = feed
        .links
        .iter()
        .find(|l| l.rel.as_deref().is_none_or(|r| r == "alternate") && l.href.starts_with("http"))
        .map(|l| l.href.clone());
    let categories: Vec<String> = feed
        .categories
        .iter()
        .flat_map(|c| std::iter::once(c.term.clone()).chain(c.subcategories.iter().map(|s| s.term.clone())))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();

    let episodes = feed.entries.iter().filter_map(|entry| episode_from_entry(entry, podcast_id)).collect();

    Ok(ParsedFeed {
        title,
        author,
        description,
        image_url,
        website,
        categories,
        episodes,
    })
}

/// Enclosure data, whether it came from `<enclosure>`/`MediaRSS` content or
/// an Atom `<link rel="enclosure">`.
struct Enclosure {
    url: String,
    mime: Option<String>,
    size: Option<u64>,
    duration: Option<std::time::Duration>,
}

fn episode_from_entry(entry: &Entry, podcast_id: &str) -> Option<Episode> {
    let enclosure = pick_enclosure(entry)?;
    let audio_url = enclosure.url;
    // feed-rs fabricates a random UUID when an item has no guid; that would
    // make a new episode on every refresh, so fall back to the stable URL.
    let guid = match entry.id.trim() {
        "" => audio_url.clone(),
        id if looks_like_generated_uuid(id) => audio_url.clone(),
        id => id.to_string(),
    };
    let media = entry.media.first();
    let title = entry
        .title
        .as_ref()
        .or_else(|| media.and_then(|m| m.title.as_ref()))
        .map(|t| plain_text(t, 300))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| guid.clone());
    let description = entry
        .summary
        .as_ref()
        .or_else(|| media.and_then(|m| m.description.as_ref()))
        .map(|t| plain_text(t, MAX_DESCRIPTION_CHARS))
        .or_else(|| entry.content.as_ref().and_then(|c| c.body.as_deref()).map(|b| strip_html(b, MAX_DESCRIPTION_CHARS)))
        .filter(|s| !s.is_empty());
    let duration_secs = media
        .and_then(|m| m.duration)
        .or(enclosure.duration)
        .map(|d| d.as_secs())
        .filter(|d| *d > 0);
    let image_url = media
        .and_then(|m| m.thumbnails.first())
        .map(|t| t.image.uri.clone())
        .filter(|u| !u.is_empty());
    let published: Option<DateTime<Utc>> = entry.published.or(entry.updated);

    Some(Episode {
        id: stable_id(&format!("{podcast_id}\n{guid}")),
        podcast_id: podcast_id.to_string(),
        guid,
        title,
        description,
        published,
        audio_url,
        mime_type: enclosure.mime,
        duration_secs,
        size_bytes: enclosure.size,
        image_url,
        position_secs: 0,
        played: false,
        local_path: None,
    })
}

fn looks_like_generated_uuid(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(&parts).all(|(len, part)| part.len() == *len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn pick_enclosure(entry: &Entry) -> Option<Enclosure> {
    let media_contents = entry.media.iter().flat_map(|m| m.content.iter()).filter_map(|c| {
        Some(Enclosure {
            url: c.url.as_ref()?.to_string(),
            mime: c.content_type.as_ref().map(ToString::to_string),
            size: c.size,
            duration: c.duration,
        })
    });
    let enclosure_links = entry
        .links
        .iter()
        .filter(|l| l.rel.as_deref() == Some("enclosure"))
        .map(|l| Enclosure {
            url: l.href.clone(),
            mime: l.media_type.clone(),
            size: l.length,
            duration: None,
        });
    let mut first_plausible = None;
    for candidate in media_contents.chain(enclosure_links) {
        if !candidate.url.starts_with("http") {
            continue;
        }
        if candidate.mime.as_deref().is_some_and(|m| m.to_ascii_lowercase().starts_with("audio/")) {
            return Some(candidate);
        }
        if first_plausible.is_none() && looks_like_audio(&candidate) {
            first_plausible = Some(candidate);
        }
    }
    first_plausible
}

/// No declared audio type: accept by extension, or when there is no type at
/// all (many feeds omit it) and it is not obviously an image/video/text.
fn looks_like_audio(enclosure: &Enclosure) -> bool {
    let path = enclosure.url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let is_media_ext = [".mp3", ".m4a", ".aac", ".ogg", ".opus", ".flac", ".wav", ".mp4"]
        .iter()
        .any(|ext| path.ends_with(ext));
    let is_other = enclosure
        .mime
        .as_deref()
        .is_some_and(|m| ["image/", "video/", "text/"].iter().any(|p| m.to_ascii_lowercase().starts_with(p)));
    is_media_ext || (enclosure.mime.is_none() && !is_other)
}

fn plain_text(text: &Text, max_chars: usize) -> String {
    // RSS descriptions are typed text/plain even when the CDATA holds HTML.
    if text.content_type.subty().as_str().contains("html") || text.content.contains('<') {
        strip_html(&text.content, max_chars)
    } else {
        truncate_chars(&collapse_whitespace(&decode_entities(&text.content)), max_chars)
    }
}

/// Drops tags, decodes the common entities and collapses whitespace.
#[must_use]
pub fn strip_html(html: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let name = tag.trim_matches('/').split_whitespace().next().unwrap_or("").trim_end_matches('/').to_ascii_lowercase();
                if matches!(name.as_str(), "p" | "br" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "ul" | "ol") {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    truncate_chars(&collapse_whitespace(&decode_entities(&out)), max_chars)
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#8217;", "\u{2019}")
        .replace("&#8216;", "\u{2018}")
        .replace("&#8220;", "\u{201c}")
        .replace("&#8221;", "\u{201d}")
        .replace("&#8230;", "\u{2026}")
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_newline = false;
    let mut pending_space = false;
    for ch in s.chars() {
        match ch {
            '\n' | '\r' => pending_newline = true,
            c if c.is_whitespace() => pending_space = true,
            c => {
                if pending_newline && !out.is_empty() {
                    out.push('\n');
                } else if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_newline = false;
                pending_space = false;
                out.push(c);
            }
        }
    }
    out
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}\u{2026}", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITUNES_FEED: &str = include_str!("../tests/fixtures/itunes_feed.xml");
    const ATOM_FEED: &str = include_str!("../tests/fixtures/atom_feed.xml");
    const SPARSE_FEED: &str = include_str!("../tests/fixtures/sparse_feed.xml");

    #[test]
    fn stable_ids_are_hex_sha1() {
        assert_eq!(stable_id("https://example.com/feed.xml"), stable_id("https://example.com/feed.xml"));
        assert_eq!(stable_id("a").len(), 40);
        assert_ne!(stable_id("a"), stable_id("b"));
        // Stored ids must not change across sha1 upgrades: FIPS 180 test vector.
        assert_eq!(stable_id("abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn parses_itunes_feed_show_and_episodes() {
        let parsed = parse_feed(ITUNES_FEED.as_bytes(), "pid").unwrap();
        assert_eq!(parsed.title, "The Example Show");
        assert_eq!(parsed.author.as_deref(), Some("Example Media"));
        assert_eq!(parsed.image_url.as_deref(), Some("https://cdn.example.com/show.jpg"));
        assert_eq!(parsed.website.as_deref(), Some("https://example.com/show"));
        assert!(parsed.categories.contains(&"Technology".to_string()));
        assert!(parsed.categories.contains(&"Tech News".to_string()));
        assert!(parsed.description.as_deref().unwrap().starts_with("A weekly show"));

        assert_eq!(parsed.episodes.len(), 3, "the item without an enclosure is skipped");
        let ep = &parsed.episodes[0];
        assert_eq!(ep.title, "Episode 42: Range requests");
        assert_eq!(ep.guid, "tag:example.com,2026:ep42");
        assert_eq!(ep.audio_url, "https://cdn.example.com/ep42.mp3");
        assert_eq!(ep.mime_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(ep.duration_secs, Some(1 * 3600 + 2 * 60 + 3));
        assert_eq!(ep.size_bytes, Some(52_428_800));
        assert_eq!(ep.image_url.as_deref(), Some("https://cdn.example.com/ep42.jpg"));
        assert_eq!(ep.published.unwrap().to_rfc3339(), "2026-09-01T10:00:00+00:00");
        let desc = ep.description.as_deref().unwrap();
        assert!(desc.starts_with("We talk about HTTP byte ranges & seeking."), "{desc}");
        assert!(!desc.contains('<'), "html stripped: {desc}");
        assert!(desc.contains("Second paragraph"));
        assert_eq!(ep.podcast_id, "pid");
        assert_eq!(ep.id, stable_id("pid\ntag:example.com,2026:ep42"));

        // Seconds-only itunes:duration and missing description.
        let ep2 = &parsed.episodes[1];
        assert_eq!(ep2.duration_secs, Some(1800));
        assert_eq!(ep2.image_url, None);
        // Audio picked over the image media in the same item.
        assert_eq!(parsed.episodes[2].audio_url, "https://cdn.example.com/ep40.m4a");
        assert_eq!(parsed.episodes[2].mime_type.as_deref(), Some("audio/x-m4a"));
    }

    #[test]
    fn parses_atom_feed_with_enclosure_links() {
        let parsed = parse_feed(ATOM_FEED.as_bytes(), "pid").unwrap();
        assert_eq!(parsed.title, "Atom Cast");
        assert_eq!(parsed.author.as_deref(), Some("Ada"));
        assert_eq!(parsed.episodes.len(), 1);
        let ep = &parsed.episodes[0];
        assert_eq!(ep.audio_url, "https://cdn.example.com/atom1.mp3");
        assert_eq!(ep.title, "First atom episode");
        assert!(ep.published.is_some());
        assert_eq!(ep.description.as_deref(), Some("Summary text."));
    }

    #[test]
    fn sparse_feed_falls_back_sensibly() {
        let parsed = parse_feed(SPARSE_FEED.as_bytes(), "pid").unwrap();
        assert_eq!(parsed.title, "Sparse");
        assert_eq!(parsed.author, None);
        assert_eq!(parsed.image_url, None);
        assert_eq!(parsed.episodes.len(), 1);
        let ep = &parsed.episodes[0];
        // No guid: the enclosure URL is the guid; no type: accepted by extension.
        assert_eq!(ep.guid, "https://cdn.example.com/sparse.mp3");
        assert_eq!(ep.duration_secs, None);
        assert_eq!(ep.description, None);
        assert!(ep.published.is_none());
    }

    #[test]
    fn generated_uuid_guids_are_recognised() {
        assert!(looks_like_generated_uuid("2c72c315-fbd2-433d-a01d-6bcea3386670"));
        assert!(!looks_like_generated_uuid("tag:example.com,2026:ep42"));
        assert!(!looks_like_generated_uuid("2c72c315-fbd2-433d-a01d"));
    }

    #[test]
    fn invalid_xml_is_an_error() {
        assert!(parse_feed(b"<html><body>not a feed</body></html>", "pid").is_err());
    }

    #[test]
    fn strip_html_handles_entities_blocks_and_length() {
        assert_eq!(strip_html("<p>Hello&nbsp;<b>world</b> &amp; you</p><p>Next</p>", 100), "Hello world & you\nNext");
        assert_eq!(strip_html("a<br/>b<br>c", 100), "a\nb\nc");
        assert_eq!(strip_html("<ul><li>one</li><li>two</li></ul>", 100), "one\ntwo");
        let long = "x".repeat(50);
        let cut = strip_html(&long, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('\u{2026}'));
        assert_eq!(strip_html("  spaced   out \n\n\n text ", 100), "spaced out\ntext");
    }
}
