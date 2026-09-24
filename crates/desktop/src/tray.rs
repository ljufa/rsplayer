//! System tray icon (desktop only).
//!
//! With `desktop_settings.close_to_tray` on, closing the window hides it and
//! playback goes on; the tray menu shows it again, drives the player, and is
//! the way to quit. Without a working tray (Linux lacking
//! libayatana-appindicator3, or a desktop that shows no tray icons and the
//! user turned the setting off) closing the window quits, as before.
//!
//! Linux app indicators deliver no click events, so every action, "Show"
//! included, lives in the menu; a left click shows the window on
//! Windows/macOS.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};

use api_models::common::{PlayerCommand, UserCommand};
use log::{info, warn};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::media_keys::CommandSlot;

/// A tray icon exists, so a hidden window can be brought back.
static TRAY_ACTIVE: AtomicBool = AtomicBool::new(false);
/// "Quit" was chosen: the next close request really closes.
static QUITTING: AtomicBool = AtomicBool::new(false);

const MAIN_WINDOW: &str = "main";

/// Create the tray icon. Failure (including the panic libappindicator raises
/// when its library is missing) only disables close-to-tray.
pub fn install(app: &AppHandle, commands: CommandSlot) {
    match catch_unwind(AssertUnwindSafe(|| build(app, commands))) {
        Ok(Ok(())) => {
            TRAY_ACTIVE.store(true, Ordering::Relaxed);
            info!("System tray icon active.");
        }
        Ok(Err(e)) => warn!("Failed to create system tray icon: {e}"),
        Err(_) => warn!("System tray unavailable (libayatana-appindicator3 missing?); closing the window quits."),
    }
}

fn build(app: &AppHandle, commands: CommandSlot) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show RSPlayer", true, None::<&str>)?;
    let prev_item = MenuItem::with_id(app, "prev", "Previous", true, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle", "Play/Pause", true, None::<&str>)?;
    let next_item = MenuItem::with_id(app, "next", "Next", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &PredefinedMenuItem::separator(app)?,
            &prev_item,
            &toggle_item,
            &next_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("RSPlayer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let cmd = match event.id().as_ref() {
                "show" => return show_main_window(app),
                "quit" => return quit(app),
                "prev" => PlayerCommand::Prev,
                "toggle" => PlayerCommand::TogglePlay,
                "next" => PlayerCommand::Next,
                _ => return,
            };
            if let Some(tx) = commands.lock().ok().and_then(|g| g.clone())
                && let Err(e) = tx.try_send(UserCommand::Player(cmd))
            {
                warn!("Tray command dropped: {e}");
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Called on the window's close request: hide it instead when the tray can
/// bring it back and the user wants that. Returns whether it was hidden.
pub fn hide_instead_of_close(window: &tauri::Window) -> bool {
    if QUITTING.load(Ordering::Relaxed) || !TRAY_ACTIVE.load(Ordering::Relaxed) || !rsplayer::close_to_tray() {
        return false;
    }
    if let Err(e) = window.hide() {
        warn!("Failed to hide window to tray: {e}");
        return false;
    }
    true
}

/// Bring the (possibly hidden or minimized) main window to the front.
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    focus(&window);
}

fn focus(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Close the window for real; the regular close path shuts the backend down.
fn quit(app: &AppHandle) {
    QUITTING.store(true, Ordering::Relaxed);
    match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => {
            if let Err(e) = window.close() {
                warn!("Failed to close window on quit: {e}");
                app.exit(0);
            }
        }
        None => app.exit(0),
    }
}
