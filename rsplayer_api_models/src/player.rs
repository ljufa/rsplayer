use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct Song {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<Duration>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub performer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,

    pub tags: HashMap<String, String>,

    pub file: String,

    pub file_date: DateTime<Utc>,
}

impl Song {
    #[must_use]
    pub fn to_json_string_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Song serialization failed!")
    }

    #[must_use]
    pub fn bytes_to_song(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    #[must_use]
    pub fn info_string(&self) -> Option<String> {
        let mut result = String::new();
        if let Some(artist) = self.artist.as_ref() {
            result.push_str(artist.as_str());
            result.push('-');
        }
        if let Some(album) = self.album.as_ref() {
            result.push_str(album.as_str());
            result.push('-');
        }
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        } else {
            result.push_str(self.file.as_str());
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
    #[must_use]
    pub fn get_title(&self) -> String {
        let mut result = String::new();
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        }
        if result.is_empty() {
            result.push_str(self.file.as_str());
        }
        result
    }
    pub fn get_file_name_without_path(&self) -> String {
        self.file.rsplit('/').next().unwrap().to_owned()
    }

    #[must_use]
    pub fn all_text(&self) -> String {
        let mut result = String::new();
        if let Some(t) = self.title.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.artist.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.album.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.album_artist.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.genre.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.composer.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.performer.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }
        if let Some(t) = self.date.as_ref() {
            result.push(' ');
            result.push_str(t.as_str());
        }

        result
    }
}
