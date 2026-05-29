use api_models::common::MetadataCommand::{self, QueryLocalFiles, RescanMetadata};
use api_models::common::MetadataLibraryItem;
use api_models::state::StateChangeEvent;

use crate::command_context::CommandContext;

pub fn handle_metadata_command(cmd: MetadataCommand, ctx: &CommandContext) {
    match cmd {
        RescanMetadata(_music_dir, full_scan) => {
            ctx.metadata_service
                .update_settings(ctx.config_store.get_settings().metadata_settings);
            let mtds = ctx.metadata_service.clone();
            let state_changes_sender = ctx.state_changes_sender.clone();
            std::thread::Builder::new()
                .name("metadata_scanner".to_string())
                .spawn(move || mtds.scan_music_dir(full_scan, &state_changes_sender))
                .expect("Failed to start metadata scanner thread");
        }
        QueryLocalFiles(dir, _) => {
            let items = ctx.metadata_service.search_local_files_by_dir(&dir);
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::SearchLocalFiles(term, limit) => {
            let items = ctx.metadata_service.search_local_files_by_dir_contains(&term, limit);
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::QueryArtists => {
            let items: Vec<MetadataLibraryItem> = ctx
                .album_repository
                .find_all_album_artists()
                .iter()
                .map(|art| MetadataLibraryItem::Artist { name: art.to_owned() })
                .collect();
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::SearchArtists(term) => {
            let items: Vec<MetadataLibraryItem> = ctx
                .album_repository
                .find_all_album_artists()
                .iter()
                .filter_map(|art| {
                    if art.to_lowercase().contains(&term.to_lowercase()) {
                        Some(MetadataLibraryItem::Artist { name: art.to_owned() })
                    } else {
                        None
                    }
                })
                .collect();
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::QueryAlbumsByArtist(artist) => {
            let items: Vec<MetadataLibraryItem> = ctx
                .album_repository
                .find_by_artist(&artist)
                .iter()
                .map(|alb| MetadataLibraryItem::Album {
                    name: alb.title.clone(),
                    id: alb.id.clone(),
                    year: alb.released,
                })
                .collect();
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::QuerySongsByAlbum(album) => {
            let items: Vec<MetadataLibraryItem> = ctx
                .album_repository
                .find_by_id(&album)
                .iter()
                .flat_map(|alb| alb.song_keys.iter().filter_map(|sk| ctx.song_repository.find_by_id(sk)))
                .map(MetadataLibraryItem::SongItem)
                .collect();
            ctx.send_event(StateChangeEvent::MetadataLocalItems(items));
        }
        MetadataCommand::LikeMediaItem(id) => {
            ctx.metadata_service.like_media_item(&id);
            ctx.send_notification(&format!("Song {id} liked"));
        }
        MetadataCommand::DislikeMediaItem(id) => {
            ctx.metadata_service.dislike_media_item(&id);
            ctx.send_notification(&format!("Song {id} disliked"));
        }
        MetadataCommand::QueryFavoriteRadioStations => {
            let favorites = ctx.metadata_service.get_favorite_radio_stations();
            ctx.send_event(StateChangeEvent::FavoriteRadioStations(favorites));
        }
        MetadataCommand::QueryLibraryStats => {
            let mut stats = ctx.metadata_service.get_library_stats();
            stats.songs_loudness_analysed = ctx.loudness_repository.count_analysed();
            ctx.send_event(StateChangeEvent::LibraryStatsEvent(stats));
        }
    }
}
