use anyhow::Result;
use api_models::{
    common::{MetadataLibraryItem, MetadataLibraryResult},
    player::Song,
    settings::MetadataStoreSettings,
    state::StateChangeEvent,
};

use mockall::automock;
use tokio::sync::broadcast::Sender;

use crate::rsp::LocalLibrary;

pub struct MetadataService {
    local: LocalLibrary,
}

#[automock]
impl MetadataService {
    pub fn new(settings: &MetadataStoreSettings) -> Result<Self> {
        let local = LocalLibrary::new(settings)?;
        Ok(Self { local })
    }

    #[allow(clippy::option_if_let_else)]
    pub fn get_all_songs_iterator(&self) -> impl Iterator<Item = Song> {
        self.local.get_all_songs_iterator()
    }

    pub fn find_song_by_id(&self, song_id: &str) -> Option<Song> {
        self.local.find_song_by_id(song_id)
    }

    pub fn get_items_by_dir(&self, dir: &str) -> MetadataLibraryResult {
        let result = self
            .local
            .get_all_songs_iterator()
            .filter(|song| song.file.starts_with(dir))
            .map(|song| {
                let Some((_, right)) = song.file.split_once(dir) else {
                    return MetadataLibraryItem::Empty;
                };
                if right.contains('/') {
                    let Some((left, _)) = right.split_once('/') else {
                        return MetadataLibraryItem::Empty;
                    };
                    return MetadataLibraryItem::Directory {
                        name: left.to_owned(),
                    };
                }
                MetadataLibraryItem::SongItem(song)
            });
        let mut result_vec: Vec<MetadataLibraryItem> = result.collect();
        result_vec.dedup();
        MetadataLibraryResult {
            items: result_vec,
            root_path: dir.to_owned(),
        }
    }

    pub fn scan_music_dir(
        &self,
        full_scan: bool,
        state_changes_sender: &Sender<StateChangeEvent>,
    ) {
        self.local
            .scan_music_dir(full_scan, state_changes_sender);
    }
}
