use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::path::PathBuf;
use std::process::exit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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
#[folder = "../../web-ui/target/dx/rsplayer_web_ui/release/web/public"]
#[exclude = "index.html"]
struct StaticContentDir;

static INDEX_HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

#[derive(Clone)]
struct AppState {
    config: Config,
    user_commands_tx: UserCommandSender,
    ws_broadcast: broadcast::Sender<Arc<String>>,
}

pub fn start(
    mut state_changes_rx: broadcast::Receiver<StateChangeEvent>,
    user_commands_tx: UserCommandSender,
    config: &Config,
) -> (
    impl Future<Output = ()>,
    Option<impl Future<Output = ()>>,
    impl Future<Output = ()>,
) {
    let (ws_broadcast, _) = broadcast::channel::<Arc<String>>(32);

    let state = AppState {
        config: config.clone(),
        user_commands_tx,
        ws_broadcast: ws_broadcast.clone(),
    };

    let app = build_router(state);

    let ws_handle = {
        let ws_broadcast = ws_broadcast;
        async move {
            loop {
                match state_changes_rx.recv().await {
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Websocket broadcaster lagged, skipped {count} messages.");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
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

    let (http_port, https_port) = get_ports();

    let http_app = app.clone();
    let http_handle = async move {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], http_port));
        if let Err(e) = axum_server::bind(addr).serve(http_app.into_make_service()).await {
            error!("HTTP server exited with error: {e}");
        }
    };

    let https_handle = if let (Ok(cert_path), Ok(key_path)) = (env::var("TLS_CERT_PATH"), env::var("TLS_CERT_KEY_PATH"))
    {
        info!("TLS enabled, starting HTTPS on port {https_port}");
        Some(async move {
            let tls_config = match RustlsConfig::from_pem_file(cert_path, key_path).await {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to load TLS config: {e}");
                    return;
                }
            };
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], https_port));
            if let Err(e) = axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service())
                .await
            {
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
        })
        .layer(cors);

    let (http_port, _) = get_ports();

    async move {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], http_port));
        if let Err(e) = axum_server::bind(addr).serve(app.into_make_service()).await {
            error!("Degraded HTTP server exited with error: {e}");
        }
    }
}

fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any);

    let cache_3d =
        SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("max-age=259200"));

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

async fn get_settings(State(state): State<AppState>) -> Json<Settings> {
    let mut settings = state.config.get_settings_mut();
    settings.version = env!("APP_VERSION").to_string();
    settings.demo_mode = env::var("DEMO_MODE").is_ok();

    #[cfg(feature = "alsa")]
    {
        let cards = hardware::audio_device::alsa::get_all_cards();
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
        settings.alsa_settings.available_audio_cards = get_cpal_audio_cards();
        settings.available_volume_control_types = vec![
            api_models::common::VolumeCrtlType::Off,
            api_models::common::VolumeCrtlType::Software,
        ];
        settings.network_mounts_available = false;
    }

    Json(settings.clone())
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
            if let Ok(name) = device.name() {
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
        let end = if file_size == 0 {
            0
        } else {
            STREAM_CHUNK.min(file_size) - 1
        };
        (0u64, end, false)
    };

    let chunk_len = (end - start + 1) as usize;

    if file_size == 0 || chunk_len == 0 {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, "0")
            .body(Body::empty())
            .unwrap();
    }

    let mut file = match tokio::fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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

fn mime_for_path(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "opus" => "audio/ogg",
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

    if range.starts_with('-') {
        let suffix: u64 = range[1..].parse().ok()?;
        let start = file_size.saturating_sub(suffix);
        Some((start, file_size - 1))
    } else if range.ends_with('-') {
        let start: u64 = range[..range.len() - 1].parse().ok()?;
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

    // SPA fallback: serve index.html with no-cache
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache, must-revalidate")
        .header("ETag", concat!("\"", env!("APP_VERSION"), "\""))
        .body(Body::from(INDEX_HTML))
        .unwrap()
}

async fn user_connected(
    ws: WebSocket,
    mut ws_rx: broadcast::Receiver<Arc<String>>,
    user_commands_tx: UserCommandSender,
) {
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

fn get_ports() -> (u16, u16) {
    let http_port = env::var("PORT")
        .unwrap_or_else(|_| "8000".to_string())
        .parse::<u16>()
        .expect("PORT is not a valid port number");

    let https_port = env::var("TLS_PORT")
        .unwrap_or_else(|_| "8143".to_string())
        .parse::<u16>()
        .expect("TLS_PORT is not a valid port number");
    (http_port, https_port)
}
