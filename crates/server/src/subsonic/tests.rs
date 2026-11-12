//! Router-level tests over a temporary fjall database.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use chrono::Utc;
use serde_json::Value as Json;
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use api_models::player::Song;
use api_models::serde_json;
use api_models::settings::{Settings, SubsonicSettings};
use config::Configuration;
use metadata::album_identity::song_album_id;
use metadata::album_repository::FjallAlbumRepository;
use metadata::metadata_service::MetadataService;
use metadata::play_statistic_repository::FjallPlayStatisticsRepository;
use metadata::playlist_service::PlaylistService;
use metadata::ports::{AlbumRepository, ArcAlbumRepository, ArcPlayStatisticsRepository, ArcSongRepository, SongRepository};
use metadata::queue_service::QueueService;
use metadata::song_repository::FjallSongRepository;
use podcast::PodcastService;

use super::auth::token;
use super::ids;
use super::{router, SubsonicState};
use crate::browse_api::BrowseState;

const SONG_KEY: &str = "Artist/Album/01 - First.flac";
const SONG_BYTES: &[u8] = b"fLaC0123456789";

struct Fixture {
    _dir: TempDir,
    app: Router,
    config: Arc<Configuration>,
}

fn song(file: &str, title: &str, track: &str) -> Song {
    Song {
        file: file.to_string(),
        title: Some(title.to_string()),
        artist: Some("Artist".to_string()),
        album_artist: Some("Artist".to_string()),
        album: Some("Album".to_string()),
        genre: Some("Rock".to_string()),
        date: Some("2001".to_string()),
        track: Some(track.to_string()),
        time: Some(Duration::from_secs(200)),
        image_id: Some("img1".to_string()),
        file_date: Utc::now(),
        ..Default::default()
    }
}

fn fixture(enabled: bool) -> Fixture {
    let dir = TempDir::new().expect("temp dir");
    let music = dir.path().join("music");
    std::fs::create_dir_all(music.join("Artist/Album")).unwrap();
    std::fs::write(music.join(SONG_KEY), SONG_BYTES).unwrap();

    let db = Arc::new(fjall::Database::builder(dir.path().join("db")).open().expect("open db"));
    let mut settings = Settings::default();
    settings.metadata_settings.music_directories = vec![music.to_string_lossy().to_string()];
    settings.subsonic_settings = SubsonicSettings {
        enabled,
        username: "alice".to_string(),
        password: "sesame".to_string(),
    };
    let config = Configuration::new(&db, settings);

    let song_repository: ArcSongRepository = Arc::new(FjallSongRepository::new(&db));
    let album_repository: ArcAlbumRepository = Arc::new(FjallAlbumRepository::new(&db));
    let statistics: ArcPlayStatisticsRepository = Arc::new(FjallPlayStatisticsRepository::new(&db));
    for s in [song(SONG_KEY, "First", "1"), song("Artist/Album/02 - Second.mp3", "Second", "2")] {
        song_repository.save(&s).unwrap();
        album_repository.update_from_song(s).unwrap();
    }
    let metadata_service = MetadataService::new(
        db.clone(),
        &config.get_settings().metadata_settings,
        song_repository.clone(),
        album_repository.clone(),
        statistics.clone(),
    )
    .unwrap();
    let (changes_tx, _) = broadcast::channel(16);
    let browse = BrowseState {
        album_repository,
        song_repository: song_repository.clone(),
        metadata_service,
        playlist_service: PlaylistService::new(&db),
        podcast_service: PodcastService::new(&db, config.clone(), changes_tx),
        queue_service: QueueService::new(&db, song_repository, statistics.clone()),
    };
    let (tx, _rx) = mpsc::channel(4);
    let app = router(SubsonicState::new(browse, statistics, config.clone(), tx));
    Fixture { _dir: dir, app, config }
}

const AUTH: &str = "u=alice&p=sesame&f=json";

async fn call(app: &Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let resp = app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    (status, to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec())
}

async fn json(app: &Router, uri: &str) -> Json {
    let (status, body) = call(app, uri).await;
    assert_eq!(status, StatusCode::OK);
    let v: Json = serde_json::from_slice(&body).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&body)));
    v["subsonic-response"].clone()
}

#[tokio::test]
async fn disabled_api_answers_with_an_error() {
    let f = fixture(false);
    let r = json(&f.app, &format!("/rest/ping?{AUTH}")).await;
    assert_eq!(r["status"], "failed");
    assert_eq!(r["error"]["code"], 0);
    // Turned on without a restart.
    let mut settings = f.config.get_settings();
    settings.subsonic_settings.enabled = true;
    f.config.save_settings(&settings);
    assert_eq!(json(&f.app, &format!("/rest/ping?{AUTH}")).await["status"], "ok");
}

#[tokio::test]
async fn authentication() {
    let f = fixture(true);
    let r = json(&f.app, "/rest/ping.view?u=alice&p=wrong&f=json").await;
    assert_eq!(r["error"]["code"], 40);
    let t = token("sesame", "abc123");
    let r = json(&f.app, &format!("/rest/ping.view?u=alice&t={t}&s=abc123&f=json&v=1.16.1&c=test")).await;
    assert_eq!(r["status"], "ok");
    let (_, xml) = call(&f.app, "/rest/ping?u=alice&p=enc:736573616d65").await;
    let xml = String::from_utf8(xml).unwrap();
    assert!(xml.contains("status=\"ok\""), "{xml}");
    // Extensions need no credentials.
    let r = json(&f.app, "/rest/getOpenSubsonicExtensions?f=json").await;
    assert_eq!(r["openSubsonicExtensions"][0]["name"], "formPost");
}

#[tokio::test]
async fn form_post() {
    let f = fixture(true);
    let req = Request::post("/rest/getAlbumList2.view")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("{AUTH}&type=newest")))
        .unwrap();
    let resp = f.app.clone().oneshot(req).await.unwrap();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Json = serde_json::from_slice(&body).unwrap();
    let albums = &v["subsonic-response"]["albumList2"]["album"];
    assert_eq!(albums.as_array().unwrap().len(), 1);
    assert_eq!(albums[0]["songCount"], 2);
    assert_eq!(albums[0]["duration"], 400);
}

#[tokio::test]
async fn browse_artist_album_song() {
    let f = fixture(true);
    let artists = json(&f.app, &format!("/rest/getArtists?{AUTH}")).await;
    let artist = &artists["artists"]["index"][0]["artist"][0];
    assert_eq!(artist["name"], "Artist");
    assert_eq!(artist["albumCount"], 1);

    let artist_id = artist["id"].as_str().unwrap();
    let r = json(&f.app, &format!("/rest/getArtist?{AUTH}&id={artist_id}")).await;
    let album_id = r["artist"]["album"][0]["id"].as_str().unwrap().to_string();

    let r = json(&f.app, &format!("/rest/getAlbum?{AUTH}&id={album_id}")).await;
    let album = &r["album"];
    assert_eq!(album["name"], "Album");
    assert_eq!(album["year"], 2001);
    let songs = album["song"].as_array().unwrap();
    assert_eq!(songs.len(), 2);
    assert_eq!(songs[0]["title"], "First");
    assert_eq!(songs[0]["track"], 1);
    assert_eq!(songs[0]["albumId"], album_id.as_str());
    assert_eq!(songs[0]["contentType"], "audio/flac");
    assert_eq!(songs[0]["isDir"], false);
    assert_eq!(songs[0]["duration"], 200);
    assert_eq!(songs[1]["suffix"], "mp3");

    let expected_album = ids::album(&song_album_id(&song(SONG_KEY, "First", "1")).unwrap());
    assert_eq!(album_id, expected_album);

    let r = json(&f.app, &format!("/rest/getAlbum?{AUTH}&id=al-bm9wZQ")).await;
    assert_eq!(r["error"]["code"], 70);
    let r = json(&f.app, &format!("/rest/getAlbum?{AUTH}")).await;
    assert_eq!(r["error"]["code"], 10);

    // Folder style.
    let r = json(&f.app, &format!("/rest/getMusicDirectory?{AUTH}&id={artist_id}")).await;
    assert_eq!(r["directory"]["child"][0]["isDir"], true);
}

#[tokio::test]
async fn get_album_as_xml() {
    let f = fixture(true);
    let album_id = ids::album(&song_album_id(&song(SONG_KEY, "First", "1")).unwrap());
    let (_, body) = call(&f.app, &format!("/rest/getAlbum?u=alice&p=sesame&id={album_id}")).await;
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains(&format!("<album id=\"{album_id}\" name=\"Album\"")), "{xml}");
    assert!(xml.contains("<song id=\"so-"), "{xml}");
    assert!(xml.ends_with("</album></subsonic-response>"), "{xml}");
}

#[tokio::test]
async fn empty_search_lists_everything_paged() {
    let f = fixture(true);
    let r = json(&f.app, &format!("/rest/search3?{AUTH}&query=%22%22&songCount=1")).await;
    assert_eq!(r["searchResult3"]["song"].as_array().unwrap().len(), 1);
    let r = json(&f.app, &format!("/rest/search3?{AUTH}&query=&songCount=10&songOffset=1")).await;
    assert_eq!(r["searchResult3"]["song"].as_array().unwrap().len(), 1);
    let r = json(&f.app, &format!("/rest/search3?{AUTH}&query=second")).await;
    assert_eq!(r["searchResult3"]["song"][0]["title"], "Second");
    assert_eq!(r["searchResult3"]["album"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn stream_whole_file_and_range() {
    let f = fixture(true);
    let id = ids::song(SONG_KEY);
    let (status, body) = call(&f.app, &format!("/rest/stream?{AUTH}&id={id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, SONG_BYTES);

    let req = Request::get(format!("/rest/download.view?{AUTH}&id={id}"))
        .header(header::RANGE, "bytes=4-7")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers()[header::CONTENT_RANGE], format!("bytes 4-7/{}", SONG_BYTES.len()));
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "audio/flac");
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"0123");

    // Not in the library.
    let r = json(&f.app, &format!("/rest/stream?{AUTH}&id={}", ids::song("../etc/passwd"))).await;
    assert_eq!(r["error"]["code"], 70);
}

#[tokio::test]
async fn star_scrobble_and_playlists() {
    let f = fixture(true);
    let id = ids::song(SONG_KEY);
    assert_eq!(json(&f.app, &format!("/rest/star?{AUTH}&id={id}")).await["status"], "ok");
    // Starring twice keeps one like.
    json(&f.app, &format!("/rest/star?{AUTH}&id={id}")).await;
    let r = json(&f.app, &format!("/rest/getStarred2?{AUTH}")).await;
    assert_eq!(r["starred2"]["song"].as_array().unwrap().len(), 1);
    assert!(r["starred2"]["song"][0]["starred"].is_string());
    json(&f.app, &format!("/rest/unstar?{AUTH}&id={id}")).await;
    let r = json(&f.app, &format!("/rest/getStarred2?{AUTH}")).await;
    assert_eq!(r["starred2"]["song"].as_array().unwrap().len(), 0);

    json(&f.app, &format!("/rest/scrobble?{AUTH}&id={id}")).await;
    json(&f.app, &format!("/rest/scrobble?{AUTH}&id={id}&submission=false")).await;
    let r = json(&f.app, &format!("/rest/getSong?{AUTH}&id={id}")).await;
    assert_eq!(r["song"]["playCount"], 1);
    let r = json(&f.app, &format!("/rest/getAlbumList2?{AUTH}&type=frequent")).await;
    assert_eq!(r["albumList2"]["album"].as_array().unwrap().len(), 1);

    let r = json(&f.app, &format!("/rest/createPlaylist?{AUTH}&name=Mix&songId={id}&songId={id}")).await;
    assert_eq!(r["playlist"]["songCount"], 2);
    let playlist_id = r["playlist"]["id"].as_str().unwrap().to_string();
    let r = json(&f.app, &format!("/rest/getPlaylists?{AUTH}")).await;
    let names: Vec<&str> = r["playlists"]["playlist"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Mix", "Most Played"]);
    json(
        &f.app,
        &format!("/rest/updatePlaylist?{AUTH}&playlistId={playlist_id}&songIndexToRemove=0"),
    )
    .await;
    let r = json(&f.app, &format!("/rest/getPlaylist?{AUTH}&id={playlist_id}")).await;
    assert_eq!(r["playlist"]["entry"].as_array().unwrap().len(), 1);
    json(&f.app, &format!("/rest/deletePlaylist?{AUTH}&id={playlist_id}")).await;
    let r = json(&f.app, &format!("/rest/getPlaylist?{AUTH}&id={playlist_id}")).await;
    assert_eq!(r["error"]["code"], 70);
}
