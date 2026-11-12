//! Library endpoints: system info, browsing (ID3 and folder style), album
//! and song lists, search, playlists and annotation.
//!
//! Everything here runs on a blocking thread (see `super::handle`): the
//! repositories read fjall synchronously and several endpoints scan the
//! whole library. Per-song play counts and likes come from the play
//! statistics, loaded once per request.

use std::cmp::Reverse;
use std::collections::HashMap;

use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use rand::seq::SliceRandom;

use api_models::common::{MetadataCommand, UserCommand};
use api_models::player::Song;
use api_models::playlist::Album;
use api_models::stat::PlayItemStatistics;
use metadata::album_identity::song_album_id;
use metadata::genre_utils::{normalize_name, resolve_id3v1_genre};

use super::ids::{self, Kind};
use super::response::{ApiError, ApiResult, Node, Value};
use super::{playlists, Params, SubsonicState};

const MAX_PAGE: usize = 500;

#[allow(clippy::too_many_lines)]
pub fn dispatch(state: &SubsonicState, endpoint: &str, p: &Params) -> ApiResult {
    match endpoint {
        // No ratings in RSPlayer: setRating is accepted so clients don't report an error.
        "ping" | "setRating" => Ok(None),
        "getLicense" => Ok(Some(Node::new("license").attr("valid", true))),
        "getOpenSubsonicExtensions" => Ok(Some(open_subsonic_extensions())),
        "getUser" => get_user(state, p),
        "getMusicFolders" => Ok(Some(Node::new("musicFolders").list(
            "musicFolder",
            vec![Node::new("musicFolder").attr("id", ids::MUSIC_FOLDER_ID).attr("name", "Music")],
        ))),
        "getScanStatus" => Ok(Some(scan_status(state))),
        "getIndexes" => Ok(Some(artist_index(state, "indexes", false))),
        "getArtists" => Ok(Some(artist_index(state, "artists", true))),
        "getArtist" => get_artist(state, p),
        "getAlbum" => get_album(state, p),
        "getSong" => get_song(state, p),
        "getMusicDirectory" => get_music_directory(state, p),
        "getGenres" => Ok(Some(genres(state))),
        "getAlbumList" => album_list(state, p, "albumList"),
        "getAlbumList2" => album_list(state, p, "albumList2"),
        "getRandomSongs" => Ok(Some(random_songs(state, p))),
        "getSongsByGenre" => songs_by_genre(state, p),
        "getStarred" => Ok(Some(starred(state, "starred"))),
        "getStarred2" => Ok(Some(starred(state, "starred2"))),
        "getArtistInfo" => Ok(Some(empty_artist_info("artistInfo"))),
        "getArtistInfo2" => Ok(Some(empty_artist_info("artistInfo2"))),
        "getAlbumInfo" | "getAlbumInfo2" => Ok(Some(Node::new("albumInfo"))),
        "getSimilarSongs" => Ok(Some(Node::new("similarSongs").list("song", Vec::new()))),
        "getSimilarSongs2" => Ok(Some(Node::new("similarSongs2").list("song", Vec::new()))),
        "getTopSongs" => Ok(Some(Node::new("topSongs").list("song", Vec::new()))),
        "getNowPlaying" => Ok(Some(Node::new("nowPlaying").list("entry", Vec::new()))),
        "search2" => Ok(Some(search(state, p, "searchResult2", false))),
        "search3" => Ok(Some(search(state, p, "searchResult3", true))),
        "getPlaylists" => Ok(Some(playlists::playlists(state))),
        "getPlaylist" => playlists::get_playlist(state, p),
        "createPlaylist" => playlists::create_playlist(state, p),
        "updatePlaylist" => playlists::update_playlist(state, p),
        "deletePlaylist" => playlists::delete_playlist(state, p),
        "star" => star(state, p, true),
        "unstar" => star(state, p, false),
        "scrobble" => scrobble(state, p),
        _ => Err(ApiError::generic(format!("Not supported by RSPlayer: {endpoint}"))),
    }
}

/// The extensions really implemented; answered without authentication.
pub fn open_subsonic_extensions() -> Node {
    Node::fragment().list(
        "openSubsonicExtensions",
        vec![Node::new("openSubsonicExtensions")
            .attr("name", "formPost")
            .values("versions", vec![Value::Int(1)])],
    )
}

// ── Entity mapping ───────────────────────────────────────────────────────────

/// Per-request view of the library with the play statistics at hand.
pub(super) struct Lib<'a> {
    state: &'a SubsonicState,
    stats: HashMap<String, PlayItemStatistics>,
}

impl<'a> Lib<'a> {
    pub(super) fn new(state: &'a SubsonicState) -> Self {
        let stats = state
            .statistics
            .get_all()
            .into_iter()
            .map(|s| (s.play_item_id.clone(), s))
            .collect();
        Self { state, stats }
    }

    fn song(&self, key: &str) -> Option<Song> {
        self.state.browse.song_repository.find_by_id(key)
    }

    fn songs_of(&self, album: &Album) -> Vec<Song> {
        let mut songs: Vec<Song> = album.song_keys.iter().filter_map(|k| self.song(k)).collect();
        songs.sort_by(|a, b| {
            (num(a.disc.as_ref()).unwrap_or(0), num(a.track.as_ref()).unwrap_or(0), &a.file).cmp(&(
                num(b.disc.as_ref()).unwrap_or(0),
                num(b.track.as_ref()).unwrap_or(0),
                &b.file,
            ))
        });
        songs
    }

    fn play_count(&self, key: &str) -> i32 {
        self.stats.get(key).map_or(0, |s| s.play_count.max(0))
    }

    fn is_liked(&self, key: &str) -> bool {
        self.stats.get(key).is_some_and(|s| s.liked_count > 0)
    }

    pub(super) fn song_node(&self, name: &'static str, song: &Song) -> Node {
        let album_id = song_album_id(song).map(|a| ids::album(&a));
        let artist = song_artist(song);
        let suffix = suffix_of(&song.file);
        let title = song
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| song.get_file_name_without_path());
        let play_count = self.play_count(&song.file);
        Node::new(name)
            .attr("id", ids::song(&song.file))
            .opt("parent", album_id.clone())
            .attr("isDir", false)
            .attr("title", title)
            .opt("album", song.album.clone())
            .opt("artist", song.artist.clone().or_else(|| artist.map(str::to_owned)))
            .opt("track", num(song.track.as_ref()))
            .opt("discNumber", num(song.disc.as_ref()))
            .opt("year", song_year(song))
            .opt("genre", genre(song.genre.as_deref()))
            .opt("coverArt", song.image_id.as_deref().map(ids::cover_art))
            .attr("contentType", content_type(&suffix))
            .attr("suffix", suffix)
            .opt("duration", song.time.map(|d| d.as_secs()))
            .attr("path", song.file.as_str())
            .opt("albumId", album_id)
            .opt("artistId", artist.map(ids::artist))
            .attr("type", "music")
            .attr("mediaType", "song")
            .attr("created", time(song.file_date))
            .opt("starred", self.is_liked(&song.file).then(|| time(song.file_date)))
            .opt("playCount", (play_count > 0).then_some(play_count))
    }

    /// An album, in the attributes of both the ID3 `album` and the folder
    /// style `child`/`directory`, with `songs` its songs.
    fn album_node(&self, name: &'static str, album: &Album, songs: &[Song]) -> Node {
        let duration: u64 = songs.iter().filter_map(|s| s.time).map(|d| d.as_secs()).sum();
        let play_count: i64 = songs.iter().map(|s| i64::from(self.play_count(&s.file))).sum();
        let artist = album.artist.as_deref().filter(|a| !a.trim().is_empty());
        Node::new(name)
            .attr("id", ids::album(&album.id))
            .attr("name", album.title.as_str())
            .attr("title", album.title.as_str())
            .attr("album", album.title.as_str())
            .attr("isDir", true)
            .opt("parent", artist.map(ids::artist))
            .opt("artist", artist)
            .opt("artistId", artist.map(ids::artist))
            .opt("coverArt", album.image_id.as_deref().map(ids::cover_art))
            .attr("songCount", songs.len())
            .attr("duration", duration)
            .opt("playCount", (play_count > 0).then_some(play_count))
            .attr("created", time(album.added))
            .opt("year", album.released.map(|r| r.year()))
            .opt("genre", genre(album.genre.as_deref()))
    }

    /// Album nodes for albums listed without their song keys (`find_all`).
    fn album_nodes(&self, name: &'static str, albums: &[Album]) -> Vec<Node> {
        albums
            .iter()
            .map(|a| {
                let full = self.state.browse.album_repository.find_by_id(&a.id).unwrap_or_else(|| a.clone());
                let songs = self.songs_of(&full);
                self.album_node(name, a, &songs)
            })
            .collect()
    }
}

/// The artist a song is listed under: the album artist, else the artist.
fn song_artist(song: &Song) -> Option<&str> {
    song.album_artist
        .as_deref()
        .or(song.artist.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// A genre as shown: `ID3v1` numbers like `(17)` resolved to their name.
fn genre(raw: Option<&str>) -> Option<String> {
    let raw = raw.map(str::trim).filter(|g| !g.is_empty())?;
    Some(resolve_id3v1_genre(raw).map_or_else(|| raw.to_string(), str::to_string))
}

/// Whether `raw` is the genre `wanted` (as shown, case-insensitive).
fn genre_is(raw: Option<&str>, wanted: &str) -> bool {
    genre(raw).is_some_and(|g| g.eq_ignore_ascii_case(wanted.trim()))
}

/// `3` from `"3"` or `"3/12"`.
fn num(value: Option<&String>) -> Option<u32> {
    value?.split('/').next()?.trim().parse().ok()
}

fn song_year(song: &Song) -> Option<i32> {
    let year = |s: &str| s.get(..4).and_then(|y| y.parse::<i32>().ok());
    song.date
        .as_deref()
        .and_then(year)
        .or_else(|| song.tags.get("year").and_then(|y| year(y)))
}

pub(super) fn time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// File extension of a library key, `wav` for cue tracks (served decoded).
pub fn suffix_of(key: &str) -> String {
    if metadata::cue_bundle::is_cue_key(key) {
        return "wav".to_string();
    }
    let name = key.rsplit('/').next().unwrap_or(key);
    name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default()
}

pub fn content_type(suffix: &str) -> &'static str {
    match suffix {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "m4a" | "m4b" | "mp4" | "alac" => "audio/mp4",
        "aif" | "aiff" | "aifc" => "audio/aiff",
        "wma" => "audio/x-ms-wma",
        "ape" => "audio/x-ape",
        "wv" => "audio/x-wavpack",
        "mpc" => "audio/x-musepack",
        "dsf" => "audio/x-dsf",
        "dff" => "audio/x-dff",
        _ => "application/octet-stream",
    }
}

fn year_of(album: &Album) -> Option<i32> {
    album.released.map(|r| r.year())
}

// ── System ───────────────────────────────────────────────────────────────────

fn get_user(state: &SubsonicState, p: &Params) -> ApiResult {
    let settings = state.config.get_settings().subsonic_settings;
    if p.get("username").is_some_and(|u| u != settings.username) {
        return Err(ApiError::not_found("User"));
    }
    Ok(Some(
        Node::new("user")
            .attr("username", settings.username)
            .attr("scrobblingEnabled", true)
            .attr("adminRole", false)
            .attr("settingsRole", false)
            .attr("downloadRole", true)
            .attr("uploadRole", false)
            .attr("playlistRole", true)
            .attr("coverArtRole", false)
            .attr("commentRole", false)
            .attr("podcastRole", false)
            .attr("streamRole", true)
            .attr("jukeboxRole", false)
            .attr("shareRole", false)
            .attr("videoConversionRole", false)
            .values("folder", vec![Value::Int(ids::MUSIC_FOLDER_ID)]),
    ))
}

fn scan_status(state: &SubsonicState) -> Node {
    state.browse.metadata_service.scan_progress().map_or_else(
        || Node::new("scanStatus").attr("scanning", false),
        |progress| Node::new("scanStatus").attr("scanning", true).attr("count", progress.done),
    )
}

/// Starts an incremental library scan, the one the web UI's "Update
/// library" runs.
pub async fn start_scan(state: &SubsonicState) -> ApiResult {
    let cmd = UserCommand::Metadata(MetadataCommand::RescanMetadata(String::new(), false));
    if state.user_commands_tx.send(cmd).await.is_err() {
        return Err(ApiError::generic("Could not start the scan"));
    }
    Ok(Some(Node::new("scanStatus").attr("scanning", true).attr("count", 0)))
}

// ── Browsing ─────────────────────────────────────────────────────────────────

/// Album artists with their album count and a cover, sorted by name.
struct ArtistEntry {
    name: String,
    album_count: usize,
    cover: Option<String>,
}

fn all_artists(state: &SubsonicState) -> Vec<ArtistEntry> {
    let mut by_key: HashMap<String, ArtistEntry> = HashMap::new();
    for album in state.browse.album_repository.find_all() {
        let Some(display) = album.artist.as_deref().map(str::trim).filter(|a| !a.is_empty()) else {
            continue;
        };
        let entry = by_key.entry(normalize_name(display)).or_insert_with(|| ArtistEntry {
            name: display.to_string(),
            album_count: 0,
            cover: None,
        });
        entry.album_count += 1;
        if entry.cover.is_none() {
            entry.cover = album.image_id;
        }
    }
    let mut artists: Vec<(String, ArtistEntry)> = by_key.into_iter().collect();
    artists.sort_by(|a, b| a.0.cmp(&b.0));
    artists.into_iter().map(|(_, e)| e).collect()
}

fn artist_node(name: &'static str, artist: &ArtistEntry) -> Node {
    Node::new(name)
        .attr("id", ids::artist(&artist.name))
        .attr("name", artist.name.as_str())
        .attr("albumCount", artist.album_count)
        .opt("coverArt", artist.cover.as_deref().map(ids::cover_art))
}

/// `getArtists` (ID3) and `getIndexes` (folders): artists grouped by
/// their first letter, `#` for anything else.
fn artist_index(state: &SubsonicState, name: &'static str, id3: bool) -> Node {
    let mut groups: Vec<(String, Vec<Node>)> = Vec::new();
    for artist in all_artists(state) {
        let letter = artist
            .name
            .chars()
            .next()
            .filter(|c| c.is_alphabetic())
            .map_or_else(|| "#".to_string(), |c| c.to_uppercase().collect());
        let node = artist_node("artist", &artist);
        match groups.iter_mut().find(|(l, _)| *l == letter) {
            Some((_, nodes)) => nodes.push(node),
            None => groups.push((letter, vec![node])),
        }
    }
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    let index = groups
        .into_iter()
        .map(|(letter, nodes)| Node::new("index").attr("name", letter).list("artist", nodes))
        .collect();
    let node = Node::new(name).attr("ignoredArticles", "");
    let node = if id3 { node } else { node.attr("lastModified", 0) };
    node.list("index", index)
}

fn artist_from(p: &Params) -> Result<String, ApiError> {
    ids::decode_as(Kind::Artist, p.required("id")?).ok_or_else(|| ApiError::not_found("Artist"))
}

fn artist_albums(state: &SubsonicState, artist: &str) -> Vec<Album> {
    let mut albums = state.browse.album_repository.find_by_artist(artist);
    albums.sort_by_key(|a| (year_of(a), a.title.to_lowercase()));
    albums
}

fn get_artist(state: &SubsonicState, p: &Params) -> ApiResult {
    let name = artist_from(p)?;
    let albums = artist_albums(state, &name);
    if albums.is_empty() {
        return Err(ApiError::not_found("Artist"));
    }
    let lib = Lib::new(state);
    let nodes: Vec<Node> = albums.iter().map(|a| lib.album_node("album", a, &lib.songs_of(a))).collect();
    let display = albums.iter().find_map(|a| a.artist.clone()).unwrap_or(name);
    let entry = ArtistEntry {
        name: display,
        album_count: albums.len(),
        cover: albums.iter().find_map(|a| a.image_id.clone()),
    };
    Ok(Some(artist_node("artist", &entry).list("album", nodes)))
}

fn album_from(state: &SubsonicState, id: &str) -> Result<Album, ApiError> {
    ids::decode_as(Kind::Album, id)
        .and_then(|key| state.browse.album_repository.find_by_id(&key))
        .ok_or_else(|| ApiError::not_found("Album"))
}

fn get_album(state: &SubsonicState, p: &Params) -> ApiResult {
    let album = album_from(state, p.required("id")?)?;
    let lib = Lib::new(state);
    let songs = lib.songs_of(&album);
    let song_nodes = songs.iter().map(|s| lib.song_node("song", s)).collect();
    Ok(Some(lib.album_node("album", &album, &songs).list("song", song_nodes)))
}

fn get_song(state: &SubsonicState, p: &Params) -> ApiResult {
    let lib = Lib::new(state);
    let song = ids::decode_as(Kind::Song, p.required("id")?)
        .and_then(|key| lib.song(&key))
        .ok_or_else(|| ApiError::not_found("Song"))?;
    Ok(Some(lib.song_node("song", &song)))
}

/// Folder browsing: an artist holds its albums, an album its songs.
fn get_music_directory(state: &SubsonicState, p: &Params) -> ApiResult {
    let id = p.required("id")?;
    let lib = Lib::new(state);
    match ids::decode(id) {
        Some((Kind::Artist, name)) => {
            let albums = artist_albums(state, &name);
            if albums.is_empty() {
                return Err(ApiError::not_found("Directory"));
            }
            let children = albums.iter().map(|a| lib.album_node("child", a, &lib.songs_of(a))).collect();
            Ok(Some(
                Node::new("directory")
                    .attr("id", id)
                    .attr("name", albums.iter().find_map(|a| a.artist.clone()).unwrap_or(name))
                    .list("child", children),
            ))
        }
        Some((Kind::Album, _)) => {
            let album = album_from(state, id)?;
            let songs = lib.songs_of(&album);
            let children = songs.iter().map(|s| lib.song_node("child", s)).collect();
            let artist = album.artist.as_deref().filter(|a| !a.trim().is_empty());
            Ok(Some(
                Node::new("directory")
                    .attr("id", id)
                    .opt("parent", artist.map(ids::artist))
                    .attr("name", album.title.as_str())
                    .list("child", children),
            ))
        }
        _ => Err(ApiError::not_found("Directory")),
    }
}

fn genres(state: &SubsonicState) -> Node {
    let mut song_counts: HashMap<String, (String, usize)> = HashMap::new();
    for song in state.browse.song_repository.find_all() {
        if let Some(genre) = genre(song.genre.as_deref()) {
            song_counts.entry(genre.to_lowercase()).or_insert_with(|| (genre, 0)).1 += 1;
        }
    }
    let mut album_counts: HashMap<String, usize> = HashMap::new();
    for album in state.browse.album_repository.find_all() {
        if let Some(genre) = genre(album.genre.as_deref()) {
            *album_counts.entry(genre.to_lowercase()).or_default() += 1;
        }
    }
    let mut list: Vec<(String, (String, usize))> = song_counts.into_iter().collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    let nodes = list
        .into_iter()
        .map(|(key, (name, songs))| {
            Node::new("genre")
                .attr("songCount", songs)
                .attr("albumCount", album_counts.get(&key).copied().unwrap_or(0))
                .text(name)
        })
        .collect();
    Node::new("genres").list("genre", nodes)
}

const fn empty_artist_info(name: &'static str) -> Node {
    Node::new(name)
}

// ── Lists ────────────────────────────────────────────────────────────────────

/// Albums with the summed play count of their songs, most played first.
fn albums_by_plays(lib: &Lib<'_>) -> Vec<Album> {
    let mut plays: HashMap<String, i64> = HashMap::new();
    for stat in lib.stats.values().filter(|s| s.play_count > 0) {
        if let Some(album_id) = lib.song(&stat.play_item_id).as_ref().and_then(song_album_id) {
            *plays.entry(album_id).or_default() += i64::from(stat.play_count);
        }
    }
    let mut ranked: Vec<(String, i64)> = plays.into_iter().collect();
    ranked.sort_by_key(|(_, count)| Reverse(*count));
    ranked
        .into_iter()
        .filter_map(|(id, _)| lib.state.browse.album_repository.find_by_id(&id))
        .collect()
}

fn album_list(state: &SubsonicState, p: &Params, name: &'static str) -> ApiResult {
    let list_type = p.required("type")?;
    let size = p.count("size", 10, MAX_PAGE);
    let offset = p.count("offset", 0, usize::MAX);
    let lib = Lib::new(state);
    let repo = &state.browse.album_repository;
    let albums: Vec<Album> = match list_type {
        "newest" => repo.find_all_sort_by_added_desc(usize::MAX),
        "alphabeticalByName" => {
            let mut all = repo.find_all();
            all.sort_by_key(|a| a.title.to_lowercase());
            all
        }
        "alphabeticalByArtist" => {
            let mut all = repo.find_all();
            all.sort_by_key(|a| (a.artist.as_deref().unwrap_or("").to_lowercase(), a.title.to_lowercase()));
            all
        }
        "random" => {
            let mut all = repo.find_all();
            all.shuffle(&mut rand::rng());
            all
        }
        // No play dates or ratings are recorded: all three rank by play count.
        "frequent" | "recent" | "highest" => albums_by_plays(&lib),
        "byYear" => {
            let from = p.int("fromYear").ok_or_else(|| ApiError::missing("fromYear"))?;
            let to = p.int("toYear").ok_or_else(|| ApiError::missing("toYear"))?;
            let (low, high) = if from <= to { (from, to) } else { (to, from) };
            let mut all: Vec<Album> = repo
                .find_all()
                .into_iter()
                .filter(|a| year_of(a).is_some_and(|y| (low..=high).contains(&i64::from(y))))
                .collect();
            all.sort_by_key(|a| year_of(a).unwrap_or(0));
            if from > to {
                all.reverse();
            }
            all
        }
        "byGenre" => {
            let genre = p.required("genre")?;
            repo.find_all()
                .into_iter()
                .filter(|a| genre_is(a.genre.as_deref(), genre))
                .collect()
        }
        // Only songs can be starred.
        "starred" => Vec::new(),
        other => return Err(ApiError::generic(format!("Unknown album list type: {other}"))),
    };
    let page: Vec<Album> = albums.into_iter().skip(offset).take(size).collect();
    let child_name = "album";
    Ok(Some(Node::new(name).list(child_name, lib.album_nodes(child_name, &page))))
}

fn random_songs(state: &SubsonicState, p: &Params) -> Node {
    let size = p.count("size", 10, MAX_PAGE);
    let genre = p.get("genre").map(str::trim).filter(|g| !g.is_empty());
    let from = p.int("fromYear");
    let to = p.int("toYear");
    let lib = Lib::new(state);
    let mut songs: Vec<Song> = state
        .browse
        .song_repository
        .find_all()
        .into_iter()
        .filter(|s| genre.is_none_or(|g| genre_is(s.genre.as_deref(), g)))
        .filter(|s| {
            let year = song_year(s).map(i64::from);
            from.is_none_or(|f| year.is_some_and(|y| y >= f)) && to.is_none_or(|t| year.is_some_and(|y| y <= t))
        })
        .collect();
    songs.shuffle(&mut rand::rng());
    songs.truncate(size);
    let nodes = songs.iter().map(|s| lib.song_node("song", s)).collect();
    Node::new("randomSongs").list("song", nodes)
}

fn songs_by_genre(state: &SubsonicState, p: &Params) -> ApiResult {
    let genre = p.required("genre")?.trim().to_string();
    let count = p.count("count", 10, MAX_PAGE);
    let offset = p.count("offset", 0, usize::MAX);
    let lib = Lib::new(state);
    let nodes = state
        .browse
        .song_repository
        .find_all()
        .into_iter()
        .filter(|s| genre_is(s.genre.as_deref(), &genre))
        .skip(offset)
        .take(count)
        .map(|s| lib.song_node("song", &s))
        .collect();
    Ok(Some(Node::new("songsByGenre").list("song", nodes)))
}

/// Liked songs. Albums and artists cannot be starred.
fn starred(state: &SubsonicState, name: &'static str) -> Node {
    let lib = Lib::new(state);
    let songs = state.browse.metadata_service.get_liked_songs(usize::MAX);
    Node::new(name)
        .list("artist", Vec::new())
        .list("album", Vec::new())
        .list("song", songs.iter().map(|s| lib.song_node("song", s)).collect())
}

// ── Search ───────────────────────────────────────────────────────────────────

/// `search2` and `search3`: case-insensitive substring match on artist
/// names, album titles and song title/artist/album. An empty query (also
/// `""`, as some clients send it) lists everything, paged: clients use it
/// to sync the whole library.
fn search(state: &SubsonicState, p: &Params, name: &'static str, id3: bool) -> Node {
    let raw = p.get("query").unwrap_or("").trim();
    let query = raw.trim_matches('"').trim().to_lowercase();
    let artist_count = p.count("artistCount", 20, MAX_PAGE);
    let artist_offset = p.count("artistOffset", 0, usize::MAX);
    let album_count = p.count("albumCount", 20, MAX_PAGE);
    let album_offset = p.count("albumOffset", 0, usize::MAX);
    let song_count = p.count("songCount", 20, MAX_PAGE);
    let song_offset = p.count("songOffset", 0, usize::MAX);
    let lib = Lib::new(state);
    let matches = |text: Option<&str>| text.is_some_and(|t| t.to_lowercase().contains(&query));

    let artists: Vec<Node> = if artist_count == 0 {
        Vec::new()
    } else {
        all_artists(state)
            .into_iter()
            .filter(|a| query.is_empty() || matches(Some(&a.name)))
            .skip(artist_offset)
            .take(artist_count)
            .map(|a| artist_node("artist", &a))
            .collect()
    };

    let albums: Vec<Node> = if album_count == 0 {
        Vec::new()
    } else {
        let page: Vec<Album> = state
            .browse
            .album_repository
            .find_all()
            .into_iter()
            .filter(|a| query.is_empty() || matches(Some(&a.title)) || matches(a.artist.as_deref()))
            .skip(album_offset)
            .take(album_count)
            .collect();
        lib.album_nodes("album", &page)
    };

    let songs: Vec<Song> = if song_count == 0 {
        Vec::new()
    } else if query.is_empty() {
        state.browse.song_repository.find_page(song_offset, song_count)
    } else {
        state
            .browse
            .song_repository
            .find_all()
            .into_iter()
            .filter(|s| matches(s.title.as_deref()) || matches(s.artist.as_deref()) || matches(s.album.as_deref()))
            .skip(song_offset)
            .take(song_count)
            .collect()
    };
    let song_nodes = songs.iter().map(|s| lib.song_node("song", s)).collect();
    // search2 lists artists as folders, which only needs id and name.
    let artists = if id3 {
        artists
    } else {
        artists
            .into_iter()
            .map(|mut a| {
                a.attrs.retain(|(k, _)| matches!(*k, "id" | "name"));
                a
            })
            .collect()
    };
    Node::new(name)
        .list("artist", artists)
        .list("album", albums)
        .list("song", song_nodes)
}

// ── Annotation ───────────────────────────────────────────────────────────────

/// Star and unstar map to the like of a song (liked while its like count
/// is above zero). Album and artist ids are accepted and ignored.
fn star(state: &SubsonicState, p: &Params, on: bool) -> ApiResult {
    for key in p.all("id").iter().filter_map(|id| ids::decode_as(Kind::Song, id)) {
        if state.browse.song_repository.find_by_id(&key).is_none() {
            continue;
        }
        let mut stat = state.statistics.find_by_id(&key).unwrap_or_else(|| PlayItemStatistics {
            play_item_id: key.clone(),
            ..Default::default()
        });
        let liked = stat.liked_count > 0;
        if liked == on {
            continue;
        }
        stat.liked_count = i32::from(on);
        if let Err(e) = state.statistics.save(&stat) {
            log::warn!("Subsonic: failed to save like of {key}: {e}");
            return Err(ApiError::generic("Could not save the star"));
        }
    }
    Ok(None)
}

/// A submitted scrobble counts as a play, like local playback does;
/// "now playing" notifications (`submission=false`) are accepted and ignored.
fn scrobble(state: &SubsonicState, p: &Params) -> ApiResult {
    let id_values = p.all("id");
    if id_values.is_empty() {
        return Err(ApiError::missing("id"));
    }
    if !p.flag("submission", true) {
        return Ok(None);
    }
    for key in id_values.iter().filter_map(|id| ids::decode_as(Kind::Song, id)) {
        if state.browse.song_repository.find_by_id(&key).is_some() {
            state.browse.metadata_service.increase_play_count(&key);
        }
    }
    Ok(None)
}
