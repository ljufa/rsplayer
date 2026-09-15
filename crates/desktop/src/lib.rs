//! Desktop and Android app: the headless server wrapped in a Tauri webview.
//!
//! [`run`] starts `rsplayer::run_backend` in-process on a free port (cwd
//! moved to a per-user data dir so the databases land there), points a
//! webview at it, and — on desktop — integrates OS media keys via souvlaki.
//! On Android the same library is loaded by the generated Kotlin host
//! (`gen/android`): the `Application` subclass sets `PORT`,
//! `RSPLAYER_DATA_DIR` and `RSPLAYER_DEFAULT_MUSIC_DIR` before the Rust entry
//! point runs, `android::early_init` registers the activity with
//! `ndk-context`, and the Kotlin `PlaybackService` provides the media session
//! over the backend's WebSocket.
//!
//! "Restart RSPlayer" from the settings UI relaunches the whole executable
//! on desktop; Android cannot relaunch its process, so the backend is
//! shut down and started again in-process instead.

#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod media_keys;

use std::env;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

use api_models::common::UserCommand;
use log::{error, info, warn};
use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, generate_context};
use tokio::sync::{mpsc, oneshot};

/// Shared slot holding the shutdown trigger of the *current* backend run.
/// Refilled by [`spawn_backend`] so the window-close handler and the restart
/// loop always reach the live backend.
type ShutdownSlot = Arc<Mutex<Option<oneshot::Sender<()>>>>;

/// One backend lifetime: the task plus the channels it reports through.
struct BackendRun {
    handle: tokio::task::JoinHandle<()>,
    /// Backend asks the wrapper to restart it ("Restart RSPlayer" in settings).
    restart_rx: mpsc::Receiver<()>,
    /// Backend hands over its command sender once up (media keys use it).
    cmd_rx: Option<oneshot::Receiver<mpsc::Sender<UserCommand>>>,
}

/// Entry point for the desktop binary and the Android host.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    android::early_init();

    // Multi-thread runtime so Tauri can block this thread with its event
    // loop while the backend runs on worker threads (what `#[tokio::main]`
    // expanded to before; on Android `run` is called from tao's own thread).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    runtime.block_on(async_main());
}

async fn async_main() {
    let data_dir = data_dir();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        error!("Failed to create data directory {:?}: {}", data_dir, e);
    }
    match env::set_current_dir(&data_dir) {
        Ok(()) => {
            info!("New work directory: {}", env::current_dir().unwrap().display());
        }
        Err(e) => {
            panic!("Failed to change work directory: {}", e);
        }
    }

    // Find a free port for the backend. Sets PORT env var so the server
    // picks up the same port via its own get_ports() logic.
    // SAFETY: called before any threads are spawned — no concurrent
    // environment access.
    let http_port = find_available_port();
    unsafe {
        env::set_var("PORT", http_port.to_string());
        env::set_var("RSPLAYER_DESKTOP", "1");
    };
    // The Kotlin media service reads the port from here (the port it
    // proposed via PORT may have been taken in the meantime).
    #[cfg(target_os = "android")]
    if let Err(e) = std::fs::write(data_dir.join("backend.port"), http_port.to_string()) {
        warn!("Failed to write backend.port: {e}");
    }

    let shutdown: ShutdownSlot = Arc::new(Mutex::new(None));
    #[allow(unused_mut)]
    let mut backend = spawn_backend(&shutdown);

    #[cfg(not(target_os = "android"))]
    media_keys::start(backend.cmd_rx.take().expect("fresh backend run has a command receiver"));

    let shutdown_on_close = Arc::clone(&shutdown);
    tauri::Builder::default()
        .setup(move |app| {
            // Create the window pointing at loading.html from the frontend
            // dist (tauri.conf.json "windows" is empty — no auto-create).
            // The loading page shows "Starting server, please wait…" with a
            // spinner — pure HTML/CSS, no WASM, visible instantly.
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(PathBuf::from("loading.html")))
                .title("RSPlayer")
                .inner_size(1200.0, 800.0)
                .build()
                .expect("failed to create window");
            redirect_when_ready(&window, http_port);

            tokio::spawn(restart_loop(backend, shutdown, app.handle().clone(), window, http_port));
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let WindowEvent::CloseRequested { .. } = event
                && let Some(tx) = shutdown_on_close.lock().ok().and_then(|mut g| g.take())
            {
                let _ = tx.send(());
            }
        })
        .run(generate_context!())
        .expect("error while running tauri application");
}

/// Where the databases and artwork cache live. Desktop: the per-user config
/// dir; Android: the app's private files dir, handed over by the Kotlin host.
fn data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        env::var_os("RSPLAYER_DATA_DIR")
            .map(PathBuf::from)
            .expect("RSPLAYER_DATA_DIR must be set by the Android host before the Rust entry point runs")
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir().map(|p| p.join("rsplayer")).unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Start the backend as a tokio task and arm `shutdown` for it.
fn spawn_backend(shutdown: &ShutdownSlot) -> BackendRun {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (cmd_sender_tx, cmd_rx) = oneshot::channel::<mpsc::Sender<UserCommand>>();
    let (restart_tx, restart_rx) = mpsc::channel::<()>(1);
    if let Ok(mut guard) = shutdown.lock() {
        *guard = Some(shutdown_tx);
    }
    let handle = tokio::spawn(async move {
        rsplayer::run_backend(Some(shutdown_rx), Some(cmd_sender_tx), Some(restart_tx)).await;
    });
    BackendRun {
        handle,
        restart_rx,
        cmd_rx: Some(cmd_rx),
    }
}

/// Background thread: poll the backend port; once it opens, redirect the
/// webview from the loading page to the real app. The redirect is repeated
/// until the webview reports the backend URL — an `eval` issued before the
/// loading page has finished loading is silently lost (seen on Android,
/// where the backend can be up before the page is).
fn redirect_when_ready(window: &WebviewWindow, http_port: u16) {
    let w = window.clone();
    spawn(move || {
        loop {
            if wait_for_backend(http_port, 30, 500) {
                break;
            }
        }
        let url = format!("http://localhost:{http_port}");
        for _ in 0..120 {
            let _ = w.eval(format!("window.location.replace('{url}')"));
            sleep(Duration::from_millis(500));
            if w.url().is_ok_and(|current| current.port() == Some(http_port)) {
                return;
            }
        }
        warn!("Webview did not navigate to {url}");
    });
}

/// Shut the backend down gracefully (database persisted, port released) and
/// wait for it, bounded.
async fn stop_backend(backend: &mut BackendRun, shutdown: &ShutdownSlot) {
    if let Some(tx) = shutdown.lock().ok().and_then(|mut g| g.take()) {
        let _ = tx.send(());
    }
    if tokio::time::timeout(Duration::from_secs(5), &mut backend.handle).await.is_err() {
        warn!("Backend did not shut down within 5s, restarting anyway");
    }
}

/// Desktop: relaunch the whole app via Tauri once the backend is down.
#[cfg(not(target_os = "android"))]
async fn restart_loop(mut backend: BackendRun, shutdown: ShutdownSlot, app_handle: AppHandle, _window: WebviewWindow, _http_port: u16) {
    if backend.restart_rx.recv().await.is_some() {
        info!("Restart requested — relaunching desktop app");
        stop_backend(&mut backend, &shutdown).await;
        app_handle.restart();
    }
}

/// Android: the process cannot relaunch itself, so start a fresh backend in
/// place and send the webview back through the loading redirect.
#[cfg(target_os = "android")]
async fn restart_loop(mut backend: BackendRun, shutdown: ShutdownSlot, _app_handle: AppHandle, window: WebviewWindow, http_port: u16) {
    while backend.restart_rx.recv().await.is_some() {
        info!("Restart requested — restarting backend in-process");
        stop_backend(&mut backend, &shutdown).await;
        backend = spawn_backend(&shutdown);
        let _ = window.eval("window.location.replace('loading.html')");
        redirect_when_ready(&window, http_port);
    }
}

fn wait_for_backend(port: u16, timeout_secs: u64, poll_interval_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    while Instant::now() < deadline {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(poll_interval_ms)) {
            Ok(_) => {
                info!("Backend is ready on port {port}");
                return true;
            }
            Err(_) => {
                sleep(Duration::from_millis(poll_interval_ms));
            }
        }
    }
    warn!("Timed out waiting for backend on port {port} after {timeout_secs}s");
    false
}

/// Try to bind a TCP listener to the given port on localhost.
fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find an available port for the backend. Prefers the `PORT` env var or
/// the default 8001. If that port is already in use, probes sequential
/// ports up to 9000, then falls back to an OS-assigned random port.
fn find_available_port() -> u16 {
    let preferred: u16 = std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8001);

    if port_is_free(preferred) {
        return preferred;
    }

    // Sequential fallback
    for port in (preferred + 1)..=9000 {
        if port_is_free(port) {
            info!("Port {preferred} is in use, using port {port} instead");
            return port;
        }
    }

    // Last resort: OS picks
    if let Ok(listener) = TcpListener::bind("127.0.0.1:0") {
        if let Ok(addr) = listener.local_addr() {
            let port = addr.port();
            drop(listener);
            info!("Falling back to OS-assigned port {port}");
            return port;
        }
    }
    panic!("Unable to find an available port for the backend");
}
