//! Desktop binary: a thin shim over the library entry point shared with the
//! Android build (see `lib.rs`).

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    rsplayer_desktop_lib::run();
}
