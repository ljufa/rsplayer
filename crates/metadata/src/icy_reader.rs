//! ICY (`SHOUTcast`) metadata extraction for radio streams.
//!
//! Wraps the HTTP body `Read` and strips the in-band metadata blocks that
//! appear every `metaint` bytes, publishing `StreamTitle` changes as
//! `CurrentSongEvent`s while handing the pure audio bytes to the decoder.

use std::io::{Read, Result as IoResult};

use api_models::state::StateChangeEvent;
use log::info;
use tokio::sync::broadcast::Sender;

use crate::radio_meta::RadioMeta;

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
                // Stations with a metadata service get their titles from `NowPlayingPoller`.
                if self.radio_meta.now_playing_url.is_none()
                    && let Some(title_part) = metadata_str.split("StreamTitle='").nth(1)
                    && let Some(title) = title_part.split("';").next().and_then(clean_stream_title)
                    && title != self.last_title
                {
                    self.last_title = title.to_string();
                    let parsed = split_stream_title(title);
                    let song = self.radio_meta.track_song(parsed.artist, parsed.title, parsed.image_url);
                    self.changes_tx.send(StateChangeEvent::CurrentSongEvent(song)).ok();
                }
            }
        }
        self.remaining = self.metaint;
        Ok(())
    }
}

/// Returns the displayable part of a `StreamTitle`, or `None` when it is junk.
///
/// Some stations (e.g. 101.ru) leak an HTTP reply into the title, like
/// `{"status":1,...}\r\n0\r\n\r\n`, so only the first line is kept and
/// JSON-looking values are dropped. Placeholders without any letter or digit,
/// like ` - `, are dropped too.
fn clean_stream_title(raw: &str) -> Option<&str> {
    // Stations leave the artist or title empty as ` - Song` or `Artist - `.
    let title = raw.lines().next()?.trim_matches(|c: char| c.is_whitespace() || c == '-');
    if title.starts_with('{') || title.starts_with('[') || !title.chars().any(char::is_alphanumeric) {
        return None;
    }
    Some(title)
}

#[derive(Debug, PartialEq, Eq)]
struct StreamTitle {
    artist: Option<String>,
    title: String,
    /// Cover art sent along with the title (iHeartRadio only).
    image_url: Option<String>,
}

/// Splits a cleaned `StreamTitle` into artist and title.
///
/// iHeartRadio stations send `ARTIST - text="Title" song_spot="M" ... amgArtworkURL="https://..."`;
/// the title and cover art are taken from those attributes. Everything else is
/// split as `Artist - Title`.
fn split_stream_title(title: &str) -> StreamTitle {
    if let Some(pos) = title.find("text=\"")
        && (pos == 0 || title[..pos].ends_with(' '))
        && let Some(text) = attribute(&title[pos..], "text")
        && !text.trim().is_empty()
    {
        let artist = title[..pos].trim_matches(|c: char| c.is_whitespace() || c == '-');
        return StreamTitle {
            artist: (!artist.is_empty()).then(|| artist.to_string()),
            title: text.trim().to_string(),
            image_url: attribute(&title[pos..], "amgArtworkURL")
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                .map(str::to_string),
        };
    }
    match title.split_once(" - ") {
        Some((artist, song)) => StreamTitle {
            artist: Some(artist.to_string()),
            title: song.to_string(),
            image_url: None,
        },
        None => StreamTitle {
            artist: None,
            title: title.to_string(),
            image_url: None,
        },
    }
}

/// Value of `name="value"` in a space separated attribute list.
fn attribute<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = attrs
        .match_indices(&key)
        .find(|(i, _)| *i == 0 || attrs[..*i].ends_with(' '))
        .map(|(i, _)| i + key.len())?;
    let len = attrs[start..].find('"')?;
    Some(&attrs[start..start + len])
}

impl<R: Read> Read for IcyMetadataReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.remaining == 0
            && let Err(e) = self.parse_metadata()
        {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(0);
            }
            return Err(e);
        }

        let read_len = std::cmp::min(buf.len(), self.remaining);
        let bytes_read = self.inner.read(&mut buf[..read_len])?;

        if bytes_read == 0 {
            return Ok(0);
        }

        self.remaining -= bytes_read;
        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamTitle, clean_stream_title, split_stream_title};

    #[test]
    fn keeps_normal_titles() {
        assert_eq!(clean_stream_title("Artist - Song"), Some("Artist - Song"));
        assert_eq!(clean_stream_title("  Song  "), Some("Song"));
        assert_eq!(clean_stream_title(" - Song"), Some("Song"));
        assert_eq!(clean_stream_title("Artist - "), Some("Artist"));
        assert_eq!(clean_stream_title("Jay-Z - Song-X"), Some("Jay-Z - Song-X"));
    }

    #[test]
    fn drops_empty_and_json_titles() {
        assert_eq!(clean_stream_title(""), None);
        assert_eq!(clean_stream_title("   "), None);
        assert_eq!(clean_stream_title(" - "), None);
        assert_eq!(clean_stream_title("-"), None);
        assert_eq!(clean_stream_title("_"), None);
        assert_eq!(
            clean_stream_title("{\"status\":1,\"message\":\"Ok\",\"result\":\"Ok\",\"errorCode\":0}\r\n0\r\n\r\n"),
            None
        );
    }

    #[test]
    fn keeps_only_first_line() {
        assert_eq!(clean_stream_title("Artist - Song\r\n0\r\n"), Some("Artist - Song"));
    }

    #[test]
    fn splits_artist_and_title() {
        assert_eq!(
            split_stream_title("Artist - Song - Remix"),
            StreamTitle {
                artist: Some("Artist".into()),
                title: "Song - Remix".into(),
                image_url: None,
            }
        );
        assert_eq!(
            split_stream_title("Station jingle"),
            StreamTitle {
                artist: None,
                title: "Station jingle".into(),
                image_url: None,
            }
        );
    }

    #[test]
    fn parses_iheart_titles() {
        let raw = r#".38 SPECIAL - text="Hold On Loosely" song_spot="M" spotInstanceId="-1" length="00:04:36" MediaBaseId="1086803" TAID="0" TPID="674619" cartcutId="700002" amgArtworkURL="https://i.iheart.com/v3/catalog/track/674619?ops=fit(200,200),format(%22jpeg%22)" spEventID="5b0f9957-1ab9-f111-83e0-025fe493a615""#;
        assert_eq!(
            split_stream_title(raw),
            StreamTitle {
                artist: Some(".38 SPECIAL".into()),
                title: "Hold On Loosely".into(),
                image_url: Some("https://i.iheart.com/v3/catalog/track/674619?ops=fit(200,200),format(%22jpeg%22)".into()),
            }
        );
    }

    #[test]
    fn iheart_without_artist_or_artwork() {
        assert_eq!(
            split_stream_title(r#"text="Station ID" song_spot="T" amgArtworkURL="null""#),
            StreamTitle {
                artist: None,
                title: "Station ID".into(),
                image_url: None,
            }
        );
    }

    #[test]
    fn plain_title_mentioning_text_is_not_iheart() {
        assert_eq!(
            split_stream_title(r#"Artist - Subtext="x""#).artist.as_deref(),
            Some("Artist")
        );
        assert_eq!(split_stream_title(r#"Artist - Subtext="x""#).title, r#"Subtext="x""#);
    }
}
