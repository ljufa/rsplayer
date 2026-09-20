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
//! "Restart RSPlayer" from the settings UI shuts the backend down and
//! relaunches the whole app: via Tauri on desktop, via the Kotlin
//! `RestartPlugin` on Android (a trampoline activity in a separate process
//! kills this one and relaunches the app). An in-process backend restart is
//! not enough — the old run's
//! spawned tasks keep serving the port with the old settings.

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
#[cfg(target_os = "android")]
use tauri::Manager;
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
    /// Backend hands over its command sender once up (desktop media keys use
    /// it; Android's media session talks to the backend over the WebSocket).
    #[cfg_attr(target_os = "android", allow(dead_code))]
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
        // First launch seeds the library with the user's Music folder, the
        // way the Android host seeds the shared Music folder.
        #[cfg(not(target_os = "android"))]
        if env::var_os("RSPLAYER_DEFAULT_MUSIC_DIR").is_none()
            && let Some(music) = dirs::audio_dir().filter(|dir| dir.is_dir())
        {
            env::set_var("RSPLAYER_DEFAULT_MUSIC_DIR", music);
        }
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
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_restart_plugin());
    builder
        .setup(move |app| {
            // Create the window pointing at loading.html from the frontend
            // dist (tauri.conf.json "windows" is empty — no auto-create).
            // The loading page shows "Starting server, please wait…" with a
            // spinner — pure HTML/CSS, no WASM, visible instantly.
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(PathBuf::from("loading.html")))
                .title("RSPlayer")
                .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
                .build()
                .expect("failed to create window");
            #[cfg(not(target_os = "android"))]
            maximize_on_small_monitor(&window);
            redirect_when_ready(&window, http_port);

            tokio::spawn(restart_loop(backend, shutdown, app.handle().clone()));
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

/// Default desktop window size (logical pixels).
const WINDOW_WIDTH: f64 = 1200.0;
const WINDOW_HEIGHT: f64 = 800.0;

/// The default window is larger than small screens (e.g. a Raspberry Pi 800×480
/// DSI panel), so part of it ends up under the desktop panel. Maximize there
/// instead and let the window manager fit it to the usable work area.
#[cfg(not(target_os = "android"))]
fn maximize_on_small_monitor(window: &WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
    if (size.width < WINDOW_WIDTH || size.height < WINDOW_HEIGHT)
        && let Err(e) = window.maximize()
    {
        warn!("Failed to maximize window on small monitor: {e}");
    }
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

/// Wait for a restart request, shut the backend down (database persisted)
/// and relaunch the whole app.
async fn restart_loop(mut backend: BackendRun, shutdown: ShutdownSlot, app_handle: AppHandle) {
    if backend.restart_rx.recv().await.is_none() {
        return;
    }
    info!("Restart requested — relaunching the app");
    stop_backend(&mut backend, &shutdown).await;
    #[cfg(not(target_os = "android"))]
    app_handle.restart();
    #[cfg(target_os = "android")]
    {
        let plugin = app_handle.state::<RestartPlugin>().0.clone();
        if let Err(e) = plugin.run_mobile_plugin_async::<serde_json::Value>("restart", ()).await {
            error!("Failed to restart the Android app: {e}");
        }
    }
}

/// Handle to the Kotlin `RestartPlugin` (`gen/android`).
#[cfg(target_os = "android")]
struct RestartPlugin(tauri::plugin::PluginHandle<tauri::Wry>);

/// Android can't relaunch its executable the way `AppHandle::restart` does on
/// desktop; the Kotlin side kills this process and relaunches the app.
#[cfg(target_os = "android")]
fn android_restart_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("rsplayer-restart")
        .setup(|app, api| {
            let handle = api.register_android_plugin("de.rsplayer.app", "RestartPlugin")?;
            app.manage(RestartPlugin(handle));
            Ok(())
        })
        .build()
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
