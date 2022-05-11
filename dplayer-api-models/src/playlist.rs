#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub name: String,
    pub id: String
}
