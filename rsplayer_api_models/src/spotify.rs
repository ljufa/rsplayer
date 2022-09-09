use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SpotifyAccountInfo {
    pub display_name: Option<String>,
    pub image_url: Option<String>,
    pub email: Option<String>,
}
