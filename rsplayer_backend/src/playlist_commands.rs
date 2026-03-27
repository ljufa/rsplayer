use api_models::common::PlaylistCommand::{
    QueryAlbumItems, QueryAlbumsByDecade, QueryAlbumsByGenre, QueryPlaylist, QueryPlaylistItems, SaveQueueAsPlaylist,
};
use api_models::playlist::PlaylistType;
use api_models::state::StateChangeEvent;

use crate::command_context::CommandContext;

#[allow(clippy::too_many_lines)]
pub fn handle_playlist_command(cmd: api_models::common::PlaylistCommand, ctx: &CommandContext) {
    match cmd {
        SaveQueueAsPlaylist(playlist_name) => {
            ctx.playlist_service
                .save_new_playlist(&playlist_name, &ctx.queue_service.get_all_songs());
            ctx.send_notification(&format!("Playlist {playlist_name} saved."));
        }
        QueryPlaylistItems(playlist_id, page_no) => {
            let songs = if playlist_id == "most_played" {
                let all = ctx.metadata_service.get_most_played_songs(100);
                all.into_iter().skip(page_no * 20).take(20).collect()
            } else if playlist_id == "liked" {
                let all = ctx.metadata_service.get_liked_songs(100);
                all.into_iter().skip(page_no * 20).take(20).collect()
            } else {
                ctx.playlist_service
                    .get_playlist_page_by_name(&playlist_id, page_no * 20, 20)
                    .items
            };
            ctx.send_event(StateChangeEvent::PlaylistItemsEvent(songs, page_no));
        }
        QueryAlbumItems(album_title, page_no) => {
            let songs = ctx.album_repository.find_by_id(&album_title).map(|alb| alb.song_keys);

            if let Some(songs) = songs {
                let songs = songs
                    .iter()
                    .skip(page_no * 20)
                    .take(20)
                    .filter_map(|song_key| ctx.song_repository.find_by_id(song_key))
                    .collect::<Vec<_>>();
                ctx.send_event(StateChangeEvent::PlaylistItemsEvent(songs, page_no));
            }
        }
        QueryPlaylist => {
            let mut pls = ctx.playlist_service.get_playlists();
            ctx.album_repository
                .find_all_sort_by_added_desc(30)
                .into_iter()
                .for_each(|alb| {
                    pls.items.push(PlaylistType::RecentlyAdded(alb));
                });
            ctx.album_repository
                .find_all_sort_by_released_desc(30)
                .into_iter()
                .for_each(|alb| {
                    pls.items.push(PlaylistType::LatestRelease(alb));
                });
            if let Some(first_most_played) = ctx.metadata_service.get_most_played_songs(1).first() {
                let pl = api_models::playlist::Playlist {
                    id: "most_played".to_string(),
                    name: "Most Played".to_string(),
                    description: Some("Your most played tracks".to_string()),
                    image: first_most_played.image_id.clone().map(|id| format!("/artwork/{id}")),
                    owner_name: None,
                };
                pls.items.push(PlaylistType::MostPlayed(pl));
            }

            if let Some(first_liked) = ctx.metadata_service.get_liked_songs(1).first() {
                let pl = api_models::playlist::Playlist {
                    id: "liked".to_string(),
                    name: "Liked".to_string(),
                    description: Some("Songs you liked".to_string()),
                    image: first_liked.image_id.clone().map(|id| format!("/artwork/{id}")),
                    owner_name: None,
                };
                pls.items.push(PlaylistType::Liked(pl));
            }

            ctx.album_repository
                .find_all_by_genre(20)
                .into_iter()
                .for_each(|(genre, albums)| {
                    pls.items.push(PlaylistType::GenreHeader(genre, albums.len()));
                });

            ctx.album_repository
                .find_all_by_decade(20)
                .into_iter()
                .for_each(|(decade, albums)| {
                    pls.items.push(PlaylistType::DecadeHeader(decade, albums.len()));
                });

            ctx.send_event(StateChangeEvent::PlaylistsEvent(pls));
        }
        QueryAlbumsByGenre(genre) => {
            let albums: Vec<api_models::playlist::Album> = ctx
                .album_repository
                .find_all_by_genre(20)
                .into_iter()
                .find(|(g, _)| *g == genre)
                .map(|(_, a)| a)
                .unwrap_or_default();
            ctx.send_event(StateChangeEvent::GenreAlbumsEvent(genre, albums));
        }
        QueryAlbumsByDecade(decade) => {
            let albums: Vec<api_models::playlist::Album> = ctx
                .album_repository
                .find_all_by_decade(20)
                .into_iter()
                .find(|(d, _)| *d == decade)
                .map(|(_, a)| a)
                .unwrap_or_default();
            ctx.send_event(StateChangeEvent::DecadeAlbumsEvent(decade, albums));
        }
    }
}
