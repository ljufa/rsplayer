//! Metadata commands: library queries, like/dislike, stats, radio stations.
//!
//! `RescanMetadata` runs the scanner on its own named thread so the command
//! loop stays responsive; everything else is answered synchronously.

use api_models::common::MetadataCommand::{self, QueryLocalFiles, RescanMetadata};
use api_models::common::MetadataLibraryItem;
use api_models::radio::RadioStation;
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
            resend_current_song_if_affected(ctx, &id);
        }
        MetadataCommand::DislikeMediaItem(id) => {
            ctx.metadata_service.dislike_media_item(&id);
            ctx.send_notification(&format!("Song {id} disliked"));
            resend_current_song_if_affected(ctx, &id);
        }
        MetadataCommand::QueryFavoriteRadioStations => send_favorite_radio_stations(ctx),
        MetadataCommand::LikeRadioStation(station) => like_radio_station(&station, ctx),
        MetadataCommand::QueryCustomRadioStations => send_custom_radio_stations(ctx),
        MetadataCommand::SaveCustomRadioStation(station) => save_custom_radio_station(&station, ctx),
        MetadataCommand::DeleteCustomRadioStation(id) => delete_custom_radio_station(&id, ctx),
        MetadataCommand::QueryLibraryStats => {
            let mut stats = ctx.metadata_service.get_library_stats();
            stats.songs_loudness_analysed = ctx.loudness_repository.count_analysed();
            ctx.send_event(StateChangeEvent::LibraryStatsEvent(stats));
        }
    }
}

fn like_radio_station(station: &RadioStation, ctx: &CommandContext) {
    // The UI also sends this to backfill details of older favorites.
    let already_liked = station
        .radio_browser_uuid
        .as_ref()
        .is_some_and(|u| ctx.metadata_service.get_favorite_radio_stations().contains(u));
    match ctx.metadata_service.like_radio_station(station) {
        Ok(saved) => {
            send_favorite_radio_stations(ctx);
            if !already_liked {
                ctx.send_notification(&format!("{} added to favorites", saved.name));
            }
        }
        Err(e) => ctx.send_error(&format!("Failed to save favorite: {e}")),
    }
}

fn save_custom_radio_station(station: &RadioStation, ctx: &CommandContext) {
    let is_new = station.id.is_empty();
    match ctx.metadata_service.save_custom_radio_station(station) {
        Ok(saved) => {
            send_custom_radio_stations(ctx);
            let what = if is_new { "added" } else { "saved" };
            ctx.send_notification(&format!("Station {} {what}", saved.name));
        }
        Err(e) => ctx.send_error(&format!("Failed to save station: {e}")),
    }
}

fn delete_custom_radio_station(id: &str, ctx: &CommandContext) {
    match ctx.metadata_service.delete_custom_radio_station(id) {
        Ok(()) => {
            send_custom_radio_stations(ctx);
            ctx.send_notification("Station removed");
        }
        Err(e) => ctx.send_error(&format!("Failed to remove station: {e}")),
    }
}

fn send_favorite_radio_stations(ctx: &CommandContext) {
    ctx.send_event(StateChangeEvent::FavoriteRadioStations(ctx.metadata_service.get_favorite_radio_stations()));
    ctx.send_event(StateChangeEvent::FavoriteRadioStationRecords(
        ctx.metadata_service.get_favorite_radio_station_records(),
    ));
}

fn send_custom_radio_stations(ctx: &CommandContext) {
    let stations = ctx.metadata_service.get_custom_radio_stations();
    ctx.send_event(StateChangeEvent::CustomRadioStationsEvent(stations));
}

/// Re-broadcast the current song with fresh statistics when it was the
/// (dis)liked item, so clients update the like indicator immediately.
fn resend_current_song_if_affected(ctx: &CommandContext, media_item_id: &str) {
    let queued_matches = ctx.queue_service.get_current_song().is_some_and(|s| s.file == media_item_id);
    if let Some(song) = ctx.current_song().filter(|s| queued_matches || s.file == media_item_id) {
        ctx.send_event(StateChangeEvent::CurrentSongEvent(song));
    }
}
