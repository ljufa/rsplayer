//! Saved playlists (read and write) and the two dynamic ones, Most
//! Played and Liked (read-only).

use api_models::player::Song;
use api_models::playlist::PlaylistType;

use super::ids::{self, Kind};
use super::library::{time, Lib};
use super::response::{ApiError, ApiResult, Node};
use super::{Params, SubsonicState};

const DYNAMIC_PLAYLIST_SONGS: usize = 100;
const MOST_PLAYED_NAME: &str = "Most Played";
const LIKED_NAME: &str = "Liked";

/// Which playlist an id names.
enum PlaylistRef {
    Saved(String),
    MostPlayed,
    Liked,
}

fn playlist_ref(id: &str) -> Option<PlaylistRef> {
    match id {
        ids::MOST_PLAYED_PLAYLIST => Some(PlaylistRef::MostPlayed),
        ids::LIKED_PLAYLIST => Some(PlaylistRef::Liked),
        _ => ids::decode_as(Kind::Playlist, id).map(PlaylistRef::Saved),
    }
}

fn saved_playlist_names(state: &SubsonicState) -> Vec<String> {
    state
        .browse
        .playlist_service
        .get_playlists()
        .items
        .into_iter()
        .filter_map(|item| match item {
            PlaylistType::Saved(pl) => Some(pl.name),
            _ => None,
        })
        .collect()
}

fn playlist_songs(state: &SubsonicState, playlist: &PlaylistRef) -> Vec<Song> {
    match playlist {
        PlaylistRef::Saved(name) => state.browse.playlist_service.get_playlist_page_by_name(name, 0, usize::MAX).items,
        PlaylistRef::MostPlayed => state.browse.metadata_service.get_most_played_songs(DYNAMIC_PLAYLIST_SONGS),
        PlaylistRef::Liked => state.browse.metadata_service.get_liked_songs(usize::MAX),
    }
}

fn playlist_node(state: &SubsonicState, playlist: &PlaylistRef, songs: &[Song]) -> Node {
    let (id, name, comment) = match playlist {
        PlaylistRef::Saved(name) => (ids::playlist(name), name.clone(), None),
        PlaylistRef::MostPlayed => (
            ids::MOST_PLAYED_PLAYLIST.to_string(),
            MOST_PLAYED_NAME.to_string(),
            Some("Your most played tracks"),
        ),
        PlaylistRef::Liked => (ids::LIKED_PLAYLIST.to_string(), LIKED_NAME.to_string(), Some("Songs you liked")),
    };
    let duration: u64 = songs.iter().filter_map(|s| s.time).map(|d| d.as_secs()).sum();
    // No dates are stored for playlists: the newest song stands in, so the
    // value only changes with the content.
    let changed = songs.iter().map(|s| s.file_date).max().unwrap_or_default();
    Node::new("playlist")
        .attr("id", id)
        .attr("name", name)
        .opt("comment", comment)
        .attr("owner", state.config.get_settings().subsonic_settings.username)
        .attr("public", false)
        .attr("songCount", songs.len())
        .attr("duration", duration)
        .attr("created", time(changed))
        .attr("changed", time(changed))
        .opt("coverArt", songs.iter().find_map(|s| s.image_id.as_deref()).map(ids::cover_art))
}

pub(super) fn playlists(state: &SubsonicState) -> Node {
    let mut refs: Vec<PlaylistRef> = saved_playlist_names(state).into_iter().map(PlaylistRef::Saved).collect();
    refs.push(PlaylistRef::MostPlayed);
    refs.push(PlaylistRef::Liked);
    let nodes = refs
        .iter()
        .filter_map(|r| {
            let songs = playlist_songs(state, r);
            let dynamic = !matches!(r, PlaylistRef::Saved(_));
            (!(dynamic && songs.is_empty())).then(|| playlist_node(state, r, &songs))
        })
        .collect();
    Node::new("playlists").list("playlist", nodes)
}

pub(super) fn get_playlist(state: &SubsonicState, p: &Params) -> ApiResult {
    let playlist = playlist_ref(p.required("id")?).ok_or_else(|| ApiError::not_found("Playlist"))?;
    if let PlaylistRef::Saved(name) = &playlist {
        if !saved_playlist_names(state).contains(name) {
            return Err(ApiError::not_found("Playlist"));
        }
    }
    let songs = playlist_songs(state, &playlist);
    let lib = Lib::new(state);
    let entries = songs.iter().map(|s| lib.song_node("entry", s)).collect();
    Ok(Some(playlist_node(state, &playlist, &songs).list("entry", entries)))
}

fn songs_from_ids(state: &SubsonicState, values: &[&str]) -> Vec<Song> {
    values
        .iter()
        .filter_map(|id| ids::decode_as(Kind::Song, id))
        .filter_map(|key| state.browse.song_repository.find_by_id(&key))
        .collect()
}

fn saved_name_from(state: &SubsonicState, id: &str) -> Result<String, ApiError> {
    match playlist_ref(id) {
        Some(PlaylistRef::Saved(name)) if saved_playlist_names(state).contains(&name) => Ok(name),
        Some(PlaylistRef::MostPlayed | PlaylistRef::Liked) => {
            Err(ApiError::generic("The Most Played and Liked playlists cannot be changed"))
        }
        _ => Err(ApiError::not_found("Playlist")),
    }
}

fn saved_playlist_node(state: &SubsonicState, name: &str) -> Node {
    let playlist = PlaylistRef::Saved(name.to_string());
    let songs = playlist_songs(state, &playlist);
    let lib = Lib::new(state);
    let entries = songs.iter().map(|s| lib.song_node("entry", s)).collect();
    playlist_node(state, &playlist, &songs).list("entry", entries)
}

/// Creates a playlist, or with `playlistId` replaces its songs. `RSPlayer`
/// stores no empty playlists, so at least one song is needed.
pub(super) fn create_playlist(state: &SubsonicState, p: &Params) -> ApiResult {
    let name = match p.get("playlistId") {
        Some(id) => saved_name_from(state, id)?,
        None => p.required("name")?.trim().to_string(),
    };
    if name.is_empty() {
        return Err(ApiError::missing("name"));
    }
    let songs = songs_from_ids(state, &p.all("songId"));
    if songs.is_empty() {
        return Err(ApiError::generic("RSPlayer cannot store an empty playlist: add at least one song"));
    }
    state.browse.playlist_service.save_new_playlist(&name, &songs);
    Ok(Some(saved_playlist_node(state, &name)))
}

/// Renames, adds and removes songs. A playlist left without songs is
/// deleted, as `RSPlayer` stores no empty playlists.
pub(super) fn update_playlist(state: &SubsonicState, p: &Params) -> ApiResult {
    let name = saved_name_from(state, p.required("playlistId")?)?;
    let service = &state.browse.playlist_service;
    let mut songs = service.get_playlist_page_by_name(&name, 0, usize::MAX).items;
    let mut remove: Vec<usize> = p.all("songIndexToRemove").iter().filter_map(|i| i.trim().parse().ok()).collect();
    remove.sort_unstable_by(|a, b| b.cmp(a));
    remove.dedup();
    for index in remove {
        if index < songs.len() {
            songs.remove(index);
        }
    }
    songs.extend(songs_from_ids(state, &p.all("songIdToAdd")));
    let new_name = p.get("name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&name).to_string();
    service.delete_playlist(&name);
    if !songs.is_empty() {
        service.save_new_playlist(&new_name, &songs);
    }
    Ok(None)
}

pub(super) fn delete_playlist(state: &SubsonicState, p: &Params) -> ApiResult {
    let name = saved_name_from(state, p.required("id")?)?;
    state.browse.playlist_service.delete_playlist(&name);
    Ok(None)
}
