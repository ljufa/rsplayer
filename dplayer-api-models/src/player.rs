use std::collections::HashMap;


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
    pub time: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub performer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    pub tags: HashMap<String, String>,

    pub file: String,
}

impl Song {
    pub fn info_string(&self) -> Option<String> {
        let mut result = "".to_string();
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
        }
        if result.is_empty() {
            result.push_str(self.file.as_str());
        }
        if !result.is_empty() {
            Some(result)
        } else {
            None
        }
    }
    pub fn get_title(&self) -> String {
        let mut result = "".to_string();
        if let Some(title) = self.title.as_ref() {
            result.push_str(title.as_str());
        }
        if result.is_empty() {
            result.push_str(self.file.as_str());
        }
        return result;
    }
}
