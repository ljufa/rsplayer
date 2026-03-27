use std::io::{Read, Result as IoResult};

use api_models::player::Song;
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
                            if album.is_empty() {
                                album = self.radio_meta.name.clone().unwrap_or_default();
                            }
                            if album.is_empty() {
                                album.clone_from(&self.radio_meta.url);
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
            return Ok(0);
        }

        self.remaining -= bytes_read;
        Ok(bytes_read)
    }
}
