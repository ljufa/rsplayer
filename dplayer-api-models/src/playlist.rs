#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct QueueItem{
    pub queue_position: u32,
    pub is_current: bool,
    pub title: String
}