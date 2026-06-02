// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    // Start the backend in a separate Tokio runtime thread
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            rsplayer::run_backend().await;
        });
    });

    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            // Point the window to the local backend server
            // In a real scenario, we might want to wait for the backend to be ready
            // or use a dynamic port.
            let _ = window.eval("window.location.replace('http://localhost:8000')");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
