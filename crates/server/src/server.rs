use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use rust_embed::RustEmbed;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
};

use api_models::common::UserCommand;
use api_models::serde_json;
use api_models::settings::Settings;
use api_models::state::StateChangeEvent;
use config::Configuration;

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);
static ACTIVE_USERS: AtomicUsize = AtomicUsize::new(0);

type Config = Arc<Configuration>;
type UserCommandSender = mpsc::Sender<UserCommand>;

#[derive(RustEmbed)]
#[folder = "../../dist/web-ui/public"]
struct StaticContentDir;

#[derive(Clone)]
struct AppState {
    config: Config,
    user_commands_tx: UserCommandSender,
    ws_broadcast: broadcast::Sender<Arc<String>>,
    /// Cached list of available output devices. Enumerating output devices is
    /// expensive and, on the Windows ASIO host, probing the drivers can disrupt
    /// the live output stream (see `get_cpal_audio_cards`). So we enumerate once
    /// (eagerly at startup, before playback runs) and reuse the result until an
    /// explicit rescan is requested via `GET /api/settings?rescan=true`.
    audio_cards_cache: Arc<Mutex<Option<Vec<api_models::common::AudioCard>>>>,
}

pub fn start(
    mut state_changes_rx: broadcast::Receiver<StateChangeEvent>,
    user_commands_tx: UserCommandSender,
    config: &Config,
) -> (impl Future<Output = ()>, Option<impl Future<Output = ()>>, impl Future<Output = ()>) {
    let (ws_broadcast, _) = broadcast::channel::<Arc<String>>(32);
    let state = AppState {
        config: config.clone(),
        user_commands_tx,
        ws_broadcast: ws_broadcast.clone(),
        // Enumerate now, while nothing is playing yet — this is the one moment
        // an ASIO driver probe cannot interrupt a live stream.
        audio_cards_cache: Arc::new(Mutex::new(Some(enumerate_audio_cards()))),
    };

    let app = build_router(state);

    let ws_handle = {
        let ws_broadcast = ws_broadcast;
        async move {
            loop {
                match state_changes_rx.recv().await {
                    Err(RecvError::Lagged(count)) => {
                        warn!("Websocket broadcaster lagged, skipped {count} messages.");
                    }
                    Err(RecvError::Closed) => {
                        error!("State change event stream closed, exiting websocket handler.");
                        break;
                    }
                    Ok(ev) => {
                        trace!("Received state changed event {ev:?}");
                        let Ok(json_msg) = serde_json::to_string(&ev) else {
                            error!("Failed to serialize state change event: {ev:?}");
                            continue;
                        };
                        if !json_msg.is_empty() && ws_broadcast.send(Arc::new(json_msg)).is_err() {
                            trace!("No active ws clients, not sending state change");
                        }
                    }
                }
            }
        }
    };

    let (http_port, https_port, bind_addr) = get_server_config();

    let http_app = app.clone();
    let http_handle = async move {
        let addr = SocketAddr::new(bind_addr, http_port);
        info!("HTTP is listening on port {http_port}");
        if let Err(e) = axum_server::bind(addr).serve(http_app.into_make_service()).await {
            error!("HTTP server exited with error: {e}");
        }
    };

    let https_handle = if let (Ok(cert_path), Ok(key_path)) = (env::var("TLS_CERT_PATH"), env::var("TLS_CERT_KEY_PATH")) {
        Some(async move {
            let tls_config = match RustlsConfig::from_pem_file(cert_path, key_path).await {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to load TLS config: {e}");
                    return;
                }
            };
            let addr = SocketAddr::new(bind_addr, https_port);
            info!("HTTPS listening on port {https_port}");
            if let Err(e) = axum_server::bind_rustls(addr, tls_config).serve(app.into_make_service()).await {
                error!("HTTPS server exited with error: {e}");
            }
        })
    } else {
        info!("TLS not configured, HTTPS disabled");
        None
    };

    (http_handle, https_handle, ws_handle)
}

pub fn start_degraded(config: &Config, error: &anyhow::Error) -> impl Future<Output = ()> {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any);

    let error_msg = error.to_string();
    let degraded_state = config.clone();

    let app = Router::new()
        .route("/api/settings", get(get_settings).post(save_settings))
        .route(
            "/api/start_error",
            get({
                let msg = error_msg;
                move || async move { msg }
            }),
        )
        .fallback(spa_or_static_fallback)
        .with_state(AppState {
            config: degraded_state,
            user_commands_tx: mpsc::channel(1).0,
            ws_broadcast: broadcast::channel(1).0,
            // Degraded mode may itself stem from an audio failure — enumerate
            // lazily on first request rather than risk a probe at startup.
            audio_cards_cache: Arc::new(Mutex::new(None)),
        })
        .layer(cors);

    let (http_port, _, bind_addr) = get_server_config();

    async move {
        let addr = SocketAddr::new(bind_addr, http_port);
        if let Err(e) = axum_server::bind(addr).serve(app.into_make_service()).await {
            error!("Degraded HTTP server exited with error: {e}");
        }
    }
}

fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any);

    let cache_3d = SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("max-age=259200"));

    let artwork = ServeDir::new("artwork");

    Router::new()
        .route("/api/ws", get(ws_handler))
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/music/{*path}", get(serve_music))
        .nest_service(
            "/artwork",
            tower::ServiceBuilder::new()
                .layer(cache_3d)
                .layer(CompressionLayer::new())
                .service(artwork),
        )
        .fallback(spa_or_static_fallback)
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| user_connected(socket, state.ws_broadcast.subscribe(), state.user_commands_tx))
}

async fn get_settings(State(state): State<AppState>, Query(query): Query<HashMap<String, String>>) -> Json<Settings> {
    let mut settings = state.config.get_settings_mut();
    settings.version = env!("CARGO_PKG_VERSION").to_string();
    settings.demo_mode = env::var("DEMO_MODE").is_ok();
    settings.desktop_mode = env::var("RSPLAYER_DESKTOP").is_ok();
    if settings.desktop_mode {
        let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
        settings.remote_access_url = local_ip().map(|ip| format!("http://{ip}:{port}"));
    }

    let force_rescan = query.get("rescan").map(String::as_str) == Some("true");
    let cards = get_cached_audio_cards(&state, force_rescan);

    #[cfg(feature = "alsa")]
    {
        if let Some(mixer_name) = &settings.volume_ctrl_settings.alsa_mixer_name {
            for card in &cards {
                if let Some(mixer) = card.mixers.iter().find(|m| &m.name == mixer_name) {
                    settings.volume_ctrl_settings.alsa_mixer = Some(mixer.clone());
                    break;
                }
            }
        }
        settings.alsa_settings.available_audio_cards = cards;
        settings.available_volume_control_types = vec![
            api_models::common::VolumeCrtlType::Off,
            api_models::common::VolumeCrtlType::Alsa,
            api_models::common::VolumeCrtlType::Pipewire,
            api_models::common::VolumeCrtlType::Software,
        ];
        settings.network_mounts_available = true;
    }

    #[cfg(not(feature = "alsa"))]
    {
        settings.alsa_settings.available_audio_cards = cards;
        settings.available_volume_control_types = vec![
            api_models::common::VolumeCrtlType::Off,
            api_models::common::VolumeCrtlType::Software,
        ];
        settings.network_mounts_available = false;
    }

    Json(settings.clone())
}

/// Return the cached audio-device list, enumerating (once) if the cache is
/// empty or a rescan was explicitly requested. Caching avoids re-probing the
/// audio backend on every settings fetch — which on the Windows ASIO host can
/// interrupt the live output stream.
fn get_cached_audio_cards(state: &AppState, force_rescan: bool) -> Vec<api_models::common::AudioCard> {
    let mut guard = state
        .audio_cards_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if force_rescan || guard.is_none() {
        *guard = Some(enumerate_audio_cards());
    }
    guard.as_ref().cloned().unwrap_or_default()
}

/// Enumerate the available output devices for the current platform/build.
/// This is the expensive/intrusive operation cached by `get_cached_audio_cards`.
fn enumerate_audio_cards() -> Vec<api_models::common::AudioCard> {
    #[cfg(feature = "alsa")]
    {
        hardware::audio_device::alsa::get_all_cards()
    }
    #[cfg(not(feature = "alsa"))]
    {
        get_cpal_audio_cards()
    }
}

#[cfg(not(feature = "alsa"))]
fn get_cpal_audio_cards() -> Vec<api_models::common::AudioCard> {
    use api_models::common::{AudioCard, PcmOutputDevice};
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let mut cards: Vec<AudioCard> = Vec::new();

    // Add an explicit "Default" entry first so the user can always pick the OS default.
    cards.push(AudioCard {
        id: "default".to_string(),
        index: -1,
        name: "System Default".to_string(),
        description: "Use the operating system default audio output".to_string(),
        pcm_devices: vec![PcmOutputDevice {
            name: "default".to_string(),
            description: "System Default".to_string(),
            card_id: "default".to_string(),
        }],
        mixers: vec![],
    });

    #[allow(deprecated)]
    if let Ok(devices) = host.output_devices() {
        for (idx, device) in devices.enumerate() {
            if let Ok(desc) = device.description() {
                let name = desc.name().to_string();
                let pcm = PcmOutputDevice {
                    name: name.clone(),
                    description: name.clone(),
                    card_id: name.clone(),
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                cards.push(AudioCard {
                    id: name.clone(),
                    index: idx as i32,
                    name: name.clone(),
                    description: name.clone(),
                    pcm_devices: vec![pcm],
                    mixers: vec![],
                });
            }
        }
    }

    // Windows ASIO drivers, listed under a separate host. Each driver is stored
    // with an `asio:` prefixed id so playback selects the ASIO host (see
    // playback::rsp::audio_host).
    #[cfg(all(target_os = "windows", feature = "asio"))]
    if let Ok(asio_host) = cpal::host_from_id(cpal::HostId::Asio) {
        #[allow(deprecated)]
        if let Ok(devices) = asio_host.output_devices() {
            for (idx, device) in devices.enumerate() {
                if let Ok(desc) = device.description() {
                    let name = desc.name().to_string();
                    let id = format!("{}{name}", playback::rsp::audio_host::ASIO_PREFIX);
                    let label = format!("{name} (ASIO)");
                    let pcm = PcmOutputDevice {
                        name: id.clone(),
                        description: label.clone(),
                        card_id: id.clone(),
                    };
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    cards.push(AudioCard {
                        id: id.clone(),
                        index: idx as i32,
                        name: label.clone(),
                        description: "ASIO".to_string(),
                        pcm_devices: vec![pcm],
                        mixers: vec![],
                    });
                }
            }
        }
    }

    cards
}

async fn save_settings(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    Json(settings): Json<Settings>,
) -> StatusCode {
    debug!("Settings to save {settings:?} and reload {query:?}");
    state.config.save_settings(&settings);
    let reload = query.get("reload").map_or("false", String::as_str);
    if reload == "true" {
        info!("Reloading service");
        // systemd should start the service again
        exit(1);
    }
    StatusCode::CREATED
}

const STREAM_CHUNK: u64 = 300 * 1024; // 300 KB per chunk

async fn serve_music(State(state): State<AppState>, AxumPath(path): AxumPath<String>, headers: HeaderMap) -> Response {
    if path.contains("..") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let range_hdr = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(std::borrow::ToOwned::to_owned);

    let music_dirs = state.config.get_settings().metadata_settings.effective_directories();
    let file_path = music_dirs
        .iter()
        .map(|dir| PathBuf::from(dir).join(&path))
        .find(|p| p.exists() && p.is_file());

    let Some(file_path) = file_path else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let file_size = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let mime_type = mime_for_path(&file_path);

    let (start, end, is_range_request) = if let Some(ref s) = range_hdr {
        match parse_range(s, file_size) {
            Some((a, b)) => {
                let max_end = a.saturating_add(STREAM_CHUNK.saturating_sub(1));
                (a, b.min(max_end), true)
            }
            None => return range_not_satisfiable(file_size),
        }
    } else {
        let end = if file_size == 0 { 0 } else { STREAM_CHUNK.min(file_size) - 1 };
        (0u64, end, false)
    };

    // Bounded by STREAM_CHUNK, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    let chunk_len = (end - start + 1) as usize;

    if file_size == 0 || chunk_len == 0 {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, "0")
            .body(Body::empty())
            .unwrap();
    }

    let Ok(mut file) = tokio::fs::File::open(&file_path).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut chunk = vec![0u8; chunk_len];
    if file.read_exact(&mut chunk).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let content_range = format!("bytes {start}-{end}/{file_size}");
    let status = if is_range_request {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, chunk_len.to_string());
    if is_range_request {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }
    builder.body(Body::from(chunk)).unwrap()
}

fn range_not_satisfiable(file_size: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
        .body(Body::empty())
        .unwrap()
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "wma" => "audio/x-ms-wma",

        _ => "application/octet-stream",
    }
}

fn parse_range(range_str: &str, file_size: u64) -> Option<(u64, u64)> {
    if file_size == 0 {
        return None;
    }
    let range_str = range_str.trim();
    if !range_str.starts_with("bytes=") {
        return None;
    }
    let ranges = &range_str[6..];
    let range = ranges.split(',').next()?.trim();

    if let Some(rest) = range.strip_prefix('-') {
        let suffix: u64 = rest.parse().ok()?;
        let start = file_size.saturating_sub(suffix);
        Some((start, file_size - 1))
    } else if let Some(rest) = range.strip_suffix('-') {
        let start: u64 = rest.parse().ok()?;
        if start >= file_size {
            return None;
        }
        Some((start, file_size - 1))
    } else {
        let mut parts = range.splitn(2, '-');
        let start: u64 = parts.next()?.parse().ok()?;
        let end: u64 = parts.next()?.parse().ok()?;
        if start >= file_size || end >= file_size || start > end {
            return None;
        }
        Some((start, end))
    }
}

async fn spa_or_static_fallback(uri: Uri, _req: Request) -> Response {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty() {
        if let Some(file) = StaticContentDir::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "max-age=259200")
                .body(Body::from(file.data.into_owned()))
                .unwrap();
        }
    }

    // SPA fallback: serve index.html from embedded dist with no-cache
    match StaticContentDir::get("index.html") {
        Some(file) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache, must-revalidate")
            .header("ETag", concat!("\"", env!("CARGO_PKG_VERSION"), "\""))
            .body(Body::from(file.data.into_owned()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("index.html not embedded — run `cargo make build_ui_release` first"))
            .unwrap(),
    }
}

async fn user_connected(ws: WebSocket, mut ws_rx: broadcast::Receiver<Arc<String>>, user_commands_tx: UserCommandSender) {
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    debug!("new websocket client: {user_id}");
    let current_users = ACTIVE_USERS.fetch_add(1, Ordering::SeqCst) + 1;
    info!("Number of active websockets is: {current_users}");

    let (mut to_user_ws, mut from_user_ws) = ws.split();

    loop {
        tokio::select! {
            Some(result) = from_user_ws.next() => {
                let msg = match result {
                    Ok(msg) => msg,
                    Err(e) => {
                        debug!("websocket error(uid={user_id}): {e}");
                        break;
                    }
                };
                match msg {
                    Message::Close(_) => break,
                    Message::Text(cmd) => {
                        let cmd = cmd.as_str();
                        if cmd.is_empty() {
                            continue;
                        }
                        info!("Got command from user {user_id}: {cmd:?}");
                        match serde_json::from_str::<UserCommand>(cmd) {
                            Ok(pc) => {
                                if user_commands_tx.send(pc).await.is_err() {
                                    error!("failed to send user message");
                                    break;
                                }
                            }
                            Err(e) => warn!("Unknown command received: [{cmd}] ({e})"),
                        }
                    }
                    _ => {}
                }
            },
            result = ws_rx.recv() => {
                match result {
                    Ok(json_msg) => {
                        if to_user_ws
                            .send(Message::text(json_msg.as_ref().clone()))
                            .await
                            .is_err()
                        {
                            debug!("Failed to send message to user {user_id}, client disconnected.");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client {user_id} is lagging, skipped {n} messages.");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    user_disconnected(user_id);
}

fn user_disconnected(my_id: usize) {
    info!("good bye user: {my_id}");
    let current_users = ACTIVE_USERS.fetch_sub(1, Ordering::SeqCst) - 1;
    info!("Number of active websockets is: {current_users}");
}

fn get_server_config() -> (u16, u16, IpAddr) {
    let http_port = env::var("PORT")
        .unwrap_or_else(|_| "8000".to_string())
        .parse::<u16>()
        .expect("PORT is not a valid port number");

    let https_port = env::var("TLS_PORT")
        .unwrap_or_else(|_| "8143".to_string())
        .parse::<u16>()
        .expect("TLS_PORT is not a valid port number");

    let bind_addr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0".to_string())
        .parse::<IpAddr>()
        .expect("BIND_ADDR is not a valid IP address");

    (http_port, https_port, bind_addr)
}

/// Returns the machine's outbound local IPv4 address by briefly connecting a
/// UDP socket to an external address (no packets are sent).
fn local_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
