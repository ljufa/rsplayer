// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

use log::{error, info, warn};
use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent, generate_context};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let config_dir = dirs::config_dir().map(|p| p.join("rsplayer")).unwrap_or_else(|| PathBuf::from("."));

    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        error!("Failed to create data directory {:?}: {}", config_dir, e);
    }
    match env::set_current_dir(&config_dir) {
        Ok(_) => {
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

    // Channel to signal the backend to shut down gracefully when the
    // desktop window is closed. Wrapped in a Mutex so it can be shared
    // between the backend spawn (setup) and the window close handler.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let shutdown_tx = Mutex::new(Some(shutdown_tx));
    // Start the backend as a tokio task (multi-thread runtime, so Tauri
    // can block the main thread with its event loop while the backend
    // runs on a worker thread — same approach as the headless server).
    tokio::spawn(async move {
        rsplayer::run_backend(Some(shutdown_rx)).await;
    });

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

            // Background thread: poll the backend port. Once it opens,
            // redirect the webview to the real app.
            let w = window.clone();
            spawn(move || {
                loop {
                    if wait_for_backend(http_port, 30, 500) {
                        let url = format!("http://localhost:{http_port}");
                        let _ = w.eval(format!("window.location.replace('{url}')"));
                        return;
                    }
                }
            });

            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let WindowEvent::CloseRequested { .. } = event
                && let Some(tx) = shutdown_tx.lock().ok().and_then(|mut g| g.take())
            {
                let _ = tx.send(());
            }
        })
        .run(generate_context!())
        .expect("error while running tauri application");
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
/// the default 8000. If that port is already in use, probes sequential
/// ports starting from 8001 up to 9000, then falls back to an OS-assigned
/// random port. Also sets the `PORT` env var so the backend picks it up.
fn find_available_port() -> u16 {
    // Preferred port: user override via PORT env, or default 8000
    let preferred: u16 = std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8000);

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
