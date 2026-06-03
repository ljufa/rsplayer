// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    // Determine the data directory for the desktop app
    let db_path = dirs::config_dir()
        .map(|p| p.join("rsplayer"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    
    // Create the directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&db_path) {
        eprintln!("Failed to create data directory {:?}: {}", db_path, e);
    }
    let db_dir = db_path.join("rsplayer.db");

    // Start the backend in a separate Tokio runtime thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            rsplayer::run_backend(Some(db_dir)).await;
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
