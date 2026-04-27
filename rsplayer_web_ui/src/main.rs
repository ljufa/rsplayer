mod dsp;
mod hooks;
pub mod lyrics;
mod page;
mod state;
pub mod vumeter;

use api_models::{
    common::{
        dur_to_string, MetadataCommand, PlayerCommand, PlaylistCommand, QueueCommand, SystemCommand, UserCommand,
    },
    state::{CurrentQueueQuery, PlayerState, StateChangeEvent},
};
use dioxus::prelude::*;
use hooks::ws_send;
use state::AppState;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::WebSocket;

use page::{
    home::HomePage, library_artists::LibraryArtistsPage, library_files::LibraryFilesPage,
    library_playlists::LibraryPlaylistsPage, library_radio::LibraryRadioPage, library_stats::LibraryStatsPage,
    not_found::NotFoundPage, player::PlayerPage, queue::QueuePage, settings::SettingsPage,
};

fn main() {
    std::panic::set_hook(Box::new(|info| {
        // Log to browser console (same as console_error_panic_hook).
        console_error_panic_hook::hook(info);
        // Inject a visible recovery overlay so the user isn't stuck on a blank screen.
        show_panic_overlay(&info.to_string());
    }));
    dioxus::launch(App);
}

/// Injects a full-screen error overlay into the DOM.
/// Uses only inline styles so it works even when the Tailwind stylesheet is unreachable.
/// Buttons use inline `onclick` JS so they continue to function after the WASM module aborts.
fn show_panic_overlay(message: &str) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Some(body) = document.body() else { return };

    let Ok(overlay) = document.create_element("div") else {
        return;
    };

    // HTML-escape the message so it displays safely inside <pre>.
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");

    overlay.set_inner_html(&format!(
        r#"<div style="position:fixed;top:0;left:0;right:0;bottom:0;z-index:99999;
                       background:#1a1a2e;color:#e2e8f0;display:flex;flex-direction:column;
                       align-items:center;justify-content:center;padding:2rem;font-family:sans-serif;">
            <div style="max-width:560px;width:100%;text-align:center;">
                <span class="material-icons" style="font-size:3rem;color:#f59e0b;">warning</span>
                <h2 style="font-size:1.4rem;font-weight:700;margin:0.75rem 0 0.5rem;">
                    Something went wrong
                </h2>
                <p style="color:#94a3b8;font-size:0.9rem;margin-bottom:1.5rem;">
                    The app encountered an unexpected error. Navigate away or reload to continue.
                </p>
                <details style="text-align:left;background:#0f172a;border:1px solid #334155;
                                border-radius:8px;padding:0.75rem 1rem;margin-bottom:1.5rem;">
                    <summary style="cursor:pointer;font-size:0.85rem;color:#94a3b8;">
                        Error details
                    </summary>
                    <pre style="margin-top:0.5rem;overflow:auto;white-space:pre-wrap;
                                font-size:0.75rem;color:#cbd5e1;">{escaped}</pre>
                </details>
                <div style="display:flex;gap:0.75rem;justify-content:center;flex-wrap:wrap;">
                    <button onclick="window.location.href='/'"
                            style="padding:0.5rem 1.25rem;background:#3b82f6;color:#fff;
                                   border:none;border-radius:6px;cursor:pointer;font-size:0.95rem;">
                        Go Home
                    </button>
                    <button onclick="window.location.reload()"
                            style="padding:0.5rem 1.25rem;background:#475569;color:#fff;
                                   border:none;border-radius:6px;cursor:pointer;font-size:0.95rem;">
                        Reload
                    </button>
                </div>
            </div>
        </div>"#
    ));

    let _ = body.append_child(&overlay);
}

// ─── UI state (modals etc.) shared via context ───────────────────────────────

const LS_VISITED_KEY: &str = "rsplayer_visited";

fn is_first_visit() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LS_VISITED_KEY).ok().flatten())
        .is_none()
}

fn mark_visited() {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(LS_VISITED_KEY, "1");
    }
}

#[derive(Clone, Copy)]
pub struct UiState {
    pub lyrics_open: Signal<bool>,
    pub shortcuts_open: Signal<bool>,
    pub welcome_open: Signal<bool>,
    pub playlist_modal_open: Signal<bool>,
    pub playlist_modal_id: Signal<Option<String>>,
    pub playlist_modal_name: Signal<String>,
    pub playlist_modal_is_album: Signal<bool>,
    pub queue_add_url_open: Signal<bool>,
    pub queue_add_url_input: Signal<String>,
    pub queue_save_playlist_open: Signal<bool>,
    pub queue_save_playlist_input: Signal<String>,
    pub queue_clear_confirm_open: Signal<bool>,
}

// ─── Navigation ─────────────────────────────────────────────────────────────

/// Newtype so `use_context` can distinguish our path signal from other `Signal<String>`s.
#[derive(Clone, Copy)]
pub struct CurrentPath(pub Signal<String>);

fn current_pathname() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_else(|| "/".to_string())
}

/// Push a new path via the History API and update the path signal (SPA navigation).
pub fn navigate(mut path_sig: Signal<String>, to: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window
            .history()
            .unwrap()
            .push_state_with_url(&JsValue::NULL, "", Some(to));
    }
    *path_sig.write() = to.to_string();
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Send a UserCommand JSON over WebSocket.
pub fn ws_user_cmd(ws: &Signal<Option<WebSocket>>, cmd: UserCommand) {
    hooks::ws_send(ws, &cmd);
}

/// Send a SystemCommand JSON over WebSocket.
pub fn send_system_cmd(ws: &Signal<Option<WebSocket>>, cmd: SystemCommand) {
    if let Some(sock) = ws.read().as_ref() {
        if let Ok(json) = serde_json::to_string(&cmd) {
            let _ = sock.send_with_str(&json);
        }
    }
}

// ─── Root App ───────────────────────────────────────────────────────────────

#[component]
fn App() -> Element {
    let app_state = AppState::new();
    use_context_provider(|| app_state.clone());
    let app_state_kb = app_state.clone();
    let ws = hooks::use_websocket(app_state);
    use_context_provider(|| ws);

    let mut path = use_signal(current_pathname);
    use_context_provider(|| CurrentPath(path));

    let ui_state = UiState {
        lyrics_open: use_signal(|| false),
        shortcuts_open: use_signal(|| false),
        welcome_open: use_signal(is_first_visit),
        playlist_modal_open: use_signal(|| false),
        playlist_modal_id: use_signal(|| None),
        playlist_modal_name: use_signal(String::new),
        playlist_modal_is_album: use_signal(|| false),
        queue_add_url_open: use_signal(|| false),
        queue_add_url_input: use_signal(String::new),
        queue_save_playlist_open: use_signal(|| false),
        queue_save_playlist_input: use_signal(String::new),
        queue_clear_confirm_open: use_signal(|| false),
    };
    use_context_provider(|| ui_state);

    // Apply theme to <html data-theme="..."> whenever it changes.
    let theme_ctx = use_context::<AppState>();
    use_effect(move || {
        let theme = theme_ctx.current_theme.read().clone();
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(html) = doc.document_element() {
                    let _ = html.set_attribute("data-theme", &theme);
                }
            }
        }
    });

    // Fetch Last.fm album art when the song has no local image.
    // Uses use_context (same pattern as child components) to get a stable signal reference.
    let app_state_ctx = use_context::<AppState>();
    use_effect(move || {
        let song = app_state_ctx.current_song.read().clone();
        let mut album_image = app_state_ctx.album_image;
        if let Some(ref s) = song {
            if album_image.peek().is_none() {
                let s = s.clone();
                spawn_local(async move {
                    album_image.set(page::player::fetch_album_cover(&s).await);
                });
            }
        } else {
            album_image.set(None);
        }
    });

    // Handle browser back / forward buttons (popstate).
    use_hook(|| {
        let closure = Closure::wrap(Box::new(move || {
            *path.write() = current_pathname();
        }) as Box<dyn FnMut()>);
        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });

    setup_keyboard_shortcuts(path, ws, app_state_kb, ui_state);

    let show_bg = *app_state_ctx.show_bg_image.read();
    let bg_style = match (show_bg, &*(app_state_ctx.album_image.read())) {
        (true, Some(url)) => format!("--bg-image: url({url});"),
        _ => String::new(),
    };

    rsx! {
        document::Stylesheet { href: asset!("/public/tw.css") }
        if (ui_state.welcome_open)() {
            WelcomeModal {}
        }
        if (ui_state.shortcuts_open)() {
            KeyboardShortcutsModal {}
        }
        if (ui_state.playlist_modal_open)() {
            PlaylistModal {}
        }
        if (ui_state.queue_add_url_open)() {
            QueueAddUrlModal {}
        }
        if (ui_state.queue_save_playlist_open)() {
            QueueSavePlaylistModal {}
        }
        if (ui_state.queue_clear_confirm_open)() {
            QueueClearConfirmModal {}
        }
        div { class: "app-shell", style: "{bg_style}",
            if show_bg && app_state_ctx.album_image.read().is_some() {
                div { class: "app-bg" }
            }
            NavBar {}
            main { class: "flex-1 backdrop-blur-md",
                {match path().as_str() {
                    "/" => rsx! { PlayerPage {} },
                    "/queue" => rsx! { QueuePage {} },
                    "/settings" => rsx! { SettingsPage {} },
                    p if p.starts_with("/library/") => rsx! {
                        LibrarySubNav {}
                        {match p {
                            "/library/files" => rsx! { LibraryFilesPage {} },
                            "/library/artists" => rsx! { LibraryArtistsPage {} },
                            "/library/radio" => rsx! { LibraryRadioPage {} },
                            "/library/playlists" => rsx! { LibraryPlaylistsPage {} },
                            "/library/stats" => rsx! { LibraryStatsPage {} },
                            other => rsx! { NotFoundPage { route: other.to_string() } },
                        }}
                    },
                    "/setup" => rsx! { HomePage {} },
                    other => rsx! { NotFoundPage { route: other.to_string() } },
                }}
            }
            FooterPlayer {}
            Notifications {}
        }
    }
}

// ─── Keyboard shortcuts ──────────────────────────────────────────────────────

fn is_typing_in_input(e: &web_sys::KeyboardEvent) -> bool {
    if let Some(target) = e.target() {
        if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
            let tag = el.tag_name().to_lowercase();
            return tag == "input" || tag == "textarea" || el.is_content_editable();
        }
    }
    false
}

/// Custom hook — must be called directly inside a component body.
fn setup_keyboard_shortcuts(
    path: Signal<String>,
    ws: Signal<Option<WebSocket>>,
    mut app_state: AppState,
    mut ui: UiState,
) {
    use_hook(|| {
        let handler = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            if is_typing_in_input(&e) {
                return;
            }
            let key = e.key();
            let shift = e.shift_key();
            let on_player = *path.peek() == "/";

            match key.as_str() {
                // ── Global navigation ──
                "1" => navigate(path, "/"),
                "2" => navigate(path, "/queue"),
                "3" => navigate(path, "/library/playlists"),
                "4" => navigate(path, "/settings"),
                // ── Library sub-pages ──
                "p" | "P" => navigate(path, "/library/playlists"),
                "f" | "F" => navigate(path, "/library/files"),
                "a" | "A" => navigate(path, "/library/artists"),
                "r" | "R" => navigate(path, "/library/radio"),
                "t" | "T" => navigate(path, "/library/stats"),
                "?" => {
                    let open = *ui.shortcuts_open.peek();
                    ui.shortcuts_open.set(!open);
                }
                "Escape" => {
                    if *ui.welcome_open.peek() {
                        mark_visited();
                        ui.welcome_open.set(false);
                    }
                    ui.shortcuts_open.set(false);
                    ui.lyrics_open.set(false);
                    ui.playlist_modal_open.set(false);
                    ui.queue_add_url_open.set(false);
                    ui.queue_save_playlist_open.set(false);
                    ui.queue_clear_confirm_open.set(false);
                }
                // ── Player shortcuts (only on / page) ──
                " " if on_player => {
                    e.prevent_default();
                    ws_send(&ws, &UserCommand::Player(PlayerCommand::TogglePlay));
                }
                "ArrowLeft" if on_player => {
                    e.prevent_default();
                    let cmd = if shift {
                        PlayerCommand::SeekBackward
                    } else {
                        PlayerCommand::Prev
                    };
                    ws_send(&ws, &UserCommand::Player(cmd));
                }
                "ArrowRight" if on_player => {
                    e.prevent_default();
                    let cmd = if shift {
                        PlayerCommand::SeekForward
                    } else {
                        PlayerCommand::Next
                    };
                    ws_send(&ws, &UserCommand::Player(cmd));
                }
                "ArrowUp" if on_player => {
                    e.prevent_default();
                    send_system_cmd(&ws, SystemCommand::VolUp);
                }
                "ArrowDown" if on_player => {
                    e.prevent_default();
                    send_system_cmd(&ws, SystemCommand::VolDown);
                }
                "m" | "M" if on_player => {
                    send_system_cmd(&ws, SystemCommand::ToggleMute);
                }
                "l" | "L" if on_player => {
                    if let Some(song) = app_state.current_song.peek().clone() {
                        let liked = song.statistics.as_ref().is_some_and(|st| st.liked_count > 0);
                        let cmd = if liked {
                            MetadataCommand::DislikeMediaItem(song.file.clone())
                        } else {
                            MetadataCommand::LikeMediaItem(song.file.clone())
                        };
                        ws_send(&ws, &UserCommand::Metadata(cmd));
                    }
                }
                "y" | "Y" if on_player => {
                    let open = *ui.lyrics_open.peek();
                    ui.lyrics_open.set(!open);
                }
                "s" | "S" if on_player => {
                    ws_send(&ws, &UserCommand::Player(PlayerCommand::CyclePlaybackMode));
                }
                "v" | "V" if on_player => {
                    let current = *app_state.visualizer_type.peek();
                    let next = current.cycle();
                    app_state.visualizer_type.set(next);
                    if let Some(window) = web_sys::window() {
                        if let Ok(Some(storage)) = window.local_storage() {
                            let _ = storage.set_item("rsplayer_visualizer", next.as_str());
                        }
                    }
                }
                _ => {}
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        }
        handler.forget();
    });
}

#[component]
fn WelcomeModal() -> Element {
    let mut ui = use_context::<UiState>();
    let CurrentPath(path) = use_context::<CurrentPath>();

    let dismiss = move |_| {
        mark_visited();
        ui.welcome_open.set(false);
    };

    let go_to_settings = move |e: Event<MouseData>| {
        e.prevent_default();
        mark_visited();
        ui.welcome_open.set(false);
        navigate(path, "/settings");
    };

    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-backdrop", onclick: dismiss }
            div { class: "modal-box max-w-lg",
                // Header
                div { class: "flex flex-col items-center text-center mb-6",
                    i { class: "material-icons text-5xl text-primary mb-2", "music_note" }
                    h2 { class: "text-2xl font-bold", "Welcome to RSPlayer" }
                    p { class: "text-base-content/60 text-sm mt-1", "Your personal music streaming server" }
                }

                // Required setup notice
                div { class: "alert alert-warning mb-5",
                    i { class: "material-icons", "warning" }
                    div {
                        p { class: "font-semibold", "Required Setup" }
                        p { class: "text-sm", "Before you can play music, configure the audio interface in Settings." }
                    }
                }

                // Steps
                div { class: "space-y-4 mb-6",
                    div { class: "flex gap-3 items-start",
                        div { class: "badge badge-primary badge-lg shrink-0 mt-0.5", "1" }
                        div {
                            div { class: "flex items-center gap-2",
                                span { class: "font-semibold", "Audio Interface" }
                                span { class: "badge badge-warning badge-sm", "Required" }
                            }
                            p { class: "text-sm text-base-content/60 mt-0.5",
                                "In Settings → Playback, select your audio interface and PCM device."
                            }
                        }
                    }
                    div { class: "flex gap-3 items-start",
                        div { class: "badge badge-secondary badge-lg shrink-0 mt-0.5", "2" }
                        div {
                            div { class: "flex items-center gap-2",
                                span { class: "font-semibold", "Music Library" }
                                span { class: "badge badge-info badge-sm", "Recommended" }
                            }
                            p { class: "text-sm text-base-content/60 mt-0.5",
                                "In Settings → Music Library, add directories containing your music files."
                            }
                        }
                    }
                    div { class: "flex gap-3 items-start",
                        div { class: "badge badge-ghost badge-lg shrink-0 mt-0.5", "3" }
                        div {
                            span { class: "font-semibold", "Start Listening" }
                            p { class: "text-sm text-base-content/60 mt-0.5",
                                "Browse your library, add songs to queue, and enjoy! Press ? for keyboard shortcuts."
                            }
                        }
                    }
                }

                // Actions
                div { class: "modal-action",
                    button { class: "btn", onclick: dismiss, "Dismiss" }
                    a {
                        class: "btn btn-primary",
                        href: "/settings",
                        onclick: go_to_settings,
                        "Go to Settings"
                    }
                }
            }
        }
    }
}

#[component]
fn KeyboardShortcutsModal() -> Element {
    let mut ui = use_context::<UiState>();
    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-backdrop", onclick: move |_| ui.shortcuts_open.set(false) }
            div { class: "modal-box max-w-md",
                button {
                    class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    onclick: move |_| ui.shortcuts_open.set(false),
                    "✕"
                }
                h3 { class: "font-bold text-lg mb-4", "Keyboard Shortcuts" }
                h4 { class: "font-semibold text-sm text-base-content/60 uppercase mb-1", "Navigation" }
                table { class: "table table-sm mb-4 w-full",
                    tbody {
                        ShortcutRow { key_label: "1 / 2 / 3 / 4", description: "Now Playing / Queue / Library / Settings" }
                        ShortcutRow { key_label: "P / F / A / R / T", description: "Playlists / Files / Artists / Radio / Stats" }
                        ShortcutRow { key_label: "?",             description: "Show / hide this help" }
                        ShortcutRow { key_label: "Esc",           description: "Close modal" }
                    }
                }
                h4 { class: "font-semibold text-sm text-base-content/60 uppercase mb-1", "Player (Now Playing page)" }
                table { class: "table table-sm w-full",
                    tbody {
                        ShortcutRow { key_label: "Space",         description: "Play / Pause" }
                        ShortcutRow { key_label: "← / →",         description: "Previous / Next track" }
                        ShortcutRow { key_label: "Shift + ← / →", description: "Seek back / forward 10 s" }
                        ShortcutRow { key_label: "↑ / ↓",         description: "Volume up / down" }
                        ShortcutRow { key_label: "M",             description: "Mute / Unmute" }
                        ShortcutRow { key_label: "L",             description: "Like / Unlike track" }
                        ShortcutRow { key_label: "Y",             description: "Toggle lyrics" }
                        ShortcutRow { key_label: "S",             description: "Cycle playback mode" }
                        ShortcutRow { key_label: "V",             description: "Cycle visualizer" }
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutRow(key_label: &'static str, description: &'static str) -> Element {
    rsx! {
        tr {
            td { class: "w-40",
                kbd { class: "kbd kbd-sm", "{key_label}" }
            }
            td { class: "text-sm", "{description}" }
        }
    }
}

#[component]
fn PlaylistModal() -> Element {
    let mut ui = use_context::<UiState>();
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let playlist_items = state.playlist_items;

    let close = move |_| {
        ui.playlist_modal_open.set(false);
        ui.playlist_modal_id.set(None);
        ui.playlist_modal_name.set(String::new());
    };

    rsx! {
        div { class: "modal modal-open modal-viewport",
            div { class: "modal-backdrop", onclick: close }
            div { class: "modal-box max-w-md",
                div { class: "flex items-center gap-2 px-4 py-3 border-b border-base-300 shrink-0",
                    h3 { class: "font-bold text-base flex-1 truncate", "{ui.playlist_modal_name}" }
                    button { class: "btn btn-xs btn-circle btn-ghost", onclick: close, "✕" }
                }
                div { class: "overflow-y-auto flex-1",
                    {playlist_items.read().iter().map(|song| {
                        let song = song.clone();
                        let file  = song.file.clone();
                        let file2 = song.file.clone();
                        let file3 = song.file.clone();
                        let title  = song.get_title();
                        let artist = song.artist.clone().unwrap_or_default();
                        let dur = song.time.as_ref().map(|t| format!(" • {}", dur_to_string(t))).unwrap_or_default();
                        rsx! {
                            div { class: "flex items-center gap-2 py-1.5 px-3 hover:bg-base-200 group",
                                div { class: "flex-1 min-w-0",
                                    p { class: "text-sm font-medium truncate", "{title}" }
                                    p { class: "text-xs text-base-content/50 truncate", "{artist}{dur}" }
                                }
                                div { class: "hidden group-hover:flex gap-1",
                                    button {
                                        class: "btn btn-ghost btn-xs",
                                        title: "Add to queue",
                                        onclick: move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongToQueue(file.clone()))),
                                        i { class: "material-icons text-sm", "queue_music" }
                                    }
                                    button {
                                        class: "btn btn-ghost btn-xs",
                                        title: "Play next",
                                        onclick: move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongAfterCurrent(file2.clone()))),
                                        i { class: "material-icons text-sm", "playlist_play" }
                                    }
                                    button {
                                        class: "btn btn-ghost btn-xs",
                                        title: "Add and play",
                                        onclick: move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongAndPlay(file3.clone()))),
                                        i { class: "material-icons text-sm", "play_arrow" }
                                    }
                                }
                            }
                        }
                    })}
                }
                div { class: "flex gap-2 px-3 py-2 border-t border-base-300 shrink-0",
                    if let Some(ref id) = *ui.playlist_modal_id.read() {
                        if *ui.playlist_modal_is_album.read() {
                            button {
                                class: "btn btn-primary btn-sm flex-1",
                                onclick: { let id = id.clone(); move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadAlbumInQueue(id.clone()))) },
                                i { class: "material-icons text-sm", "playlist_play" }
                                "Load"
                            }
                            button {
                                class: "btn btn-sm flex-1",
                                onclick: { let id = id.clone(); move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddAlbumToQueue(id.clone()))) },
                                i { class: "material-icons text-sm", "queue" }
                                "Add"
                            }
                        } else {
                            button {
                                class: "btn btn-primary btn-sm flex-1",
                                onclick: { let id = id.clone(); move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadPlaylistInQueue(id.clone()))) },
                                i { class: "material-icons text-sm", "playlist_play" }
                                "Load"
                            }
                            button {
                                class: "btn btn-sm flex-1",
                                onclick: { let id = id.clone(); move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddPlaylistToQueue(id.clone()))) },
                                i { class: "material-icons text-sm", "queue" }
                                "Add"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QueueAddUrlModal() -> Element {
    let mut ui = use_context::<UiState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let close = move |_| {
        ui.queue_add_url_open.set(false);
        ui.queue_add_url_input.set(String::new());
    };

    rsx! {
        div { class: "modal modal-open modal-viewport",
            div { class: "modal-backdrop", onclick: close }
            div { class: "modal-box",
                button {
                    class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    onclick: close,
                    "✕"
                }
                h3 { class: "font-bold text-lg mb-4", "Add streaming URL(s)" }
                textarea {
                    class: "textarea textarea-bordered w-full",
                    placeholder: "Enter one URL per line",
                    autofocus: true,
                    oninput: move |e| ui.queue_add_url_input.set(e.value()),
                }
                div { class: "modal-action",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let val = ui.queue_add_url_input.read().clone();
                            if val.len() > 3 {
                                for line in val.lines() {
                                    if line.len() > 5{
                                        ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongToQueue(line.to_string())));
                                    }
                                }
                            }
                            ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                CurrentQueueQuery::WithSearchTerm(String::new(), 0)
                            )));
                            ui.queue_add_url_open.set(false);
                            ui.queue_add_url_input.set(String::new());
                        },
                        "Add"
                    }
                    button {
                        class: "btn",
                        onclick: close,
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn QueueSavePlaylistModal() -> Element {
    let mut ui = use_context::<UiState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let close = move |_| {
        ui.queue_save_playlist_open.set(false);
        ui.queue_save_playlist_input.set(String::new());
    };

    rsx! {
        div { class: "modal modal-open modal-viewport",
            div { class: "modal-backdrop", onclick: close }
            div { class: "modal-box",
                button {
                    class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    onclick: close,
                    "✕"
                }
                h3 { class: "font-bold text-lg mb-4", "Save as playlist" }
                input {
                    class: "input input-bordered w-full",
                    placeholder: "Playlist name",
                    autofocus: true,
                    oninput: move |e| ui.queue_save_playlist_input.set(e.value()),
                }
                div { class: "modal-action",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let name = ui.queue_save_playlist_input.read().clone();
                            if name.len() > 3 {
                                ws_send(&ws, &UserCommand::Playlist(
                                    PlaylistCommand::SaveQueueAsPlaylist(name)
                                ));
                                ui.queue_save_playlist_open.set(false);
                                ui.queue_save_playlist_input.set(String::new());
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "btn",
                        onclick: close,
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
fn QueueClearConfirmModal() -> Element {
    let mut ui = use_context::<UiState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let close = move |_| {
        ui.queue_clear_confirm_open.set(false);
    };

    rsx! {
        div { class: "modal modal-open modal-viewport",
            div { class: "modal-backdrop", onclick: close }
            div { class: "modal-box",
                h3 { class: "font-bold text-lg", "Clear queue?" }
                p { class: "py-4", "This will remove all items from the current queue." }
                div { class: "modal-action",
                    button {
                        class: "btn btn-warning",
                        onclick: move |_| {
                            ws_send(&ws, &UserCommand::Queue(QueueCommand::ClearQueue));
                            ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                CurrentQueueQuery::WithSearchTerm(String::new(), 0)
                            )));
                            ui.queue_clear_confirm_open.set(false);
                        },
                        "Confirm"
                    }
                    button {
                        class: "btn",
                        onclick: close,
                        "Cancel"
                    }
                }
            }
        }
    }
}

// ─── SPA link component ─────────────────────────────────────────────────────

#[component]
pub fn NavLink(to: String, #[props(optional)] class: Option<String>, children: Element) -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    let cls = class.unwrap_or_default();
    let to_nav = to.clone();
    rsx! {
        a {
            class: "{cls}",
            href: "{to}",
            onclick: move |e| {
                e.prevent_default();
                navigate(path, &to_nav);
            },
            {children}
        }
    }
}

// ─── Navigation bar ─────────────────────────────────────────────────────────

#[component]
fn NavBar() -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    let current = path();
    let is_library = current.starts_with("/library/");

    rsx! {
        nav { class: "app-nav",
            ul { class: "app-nav__items",
                NavItem { label: "Now Playing", icon: "music_note",    active: current == "/",         to: "/" }
                NavItem { label: "Queue",       icon: "queue_music",   active: current == "/queue",    to: "/queue" }
                NavItem { label: "Library",     icon: "library_music", active: is_library,             to: "/library/playlists" }
                NavItem { label: "Settings",    icon: "tune",          active: current == "/settings", to: "/settings" }
            }
        }
    }
}

// ── Library sub-navigation (rendered at page-content level) ─────────────────

#[component]
fn LibrarySubNav() -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    let current = path();
    rsx! {
        div { class: "flex gap-2 px-3 py-2 border-b border-base-300 overflow-x-auto",
            for (label, to) in [
                ("Playlists", "/library/playlists"),
                ("Files",     "/library/files"),
                ("Artists",   "/library/artists"),
                ("Radio",     "/library/radio"),
                ("Stats",     "/library/stats"),
            ] {
                button {
                    class: if current == to { "btn btn-sm btn-primary" } else { "btn btn-sm btn-ghost" },
                    onclick: move |_| navigate(path, to),
                    "{label}"
                }
            }
        }
    }
}

#[component]
fn NavItem(label: &'static str, icon: &'static str, active: bool, to: &'static str) -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    rsx! {
        li { class: if active { "app-nav__item is-active" } else { "app-nav__item" },
            a {
                class: "app-nav__link",
                href: "{to}",
                onclick: move |e| {
                    e.prevent_default();
                    navigate(path, to);
                },
                i { class: "material-icons", aria_hidden: "true", "{icon}" }
                span { class: "app-nav__label", "{label}" }
            }
            if active {
                div { class: "h-0.5 bg-primary rounded-full mx-3" }
            }
        }
    }
}

// ─── Footer mini-player (hidden on Player page) ────────────────────────────

#[component]
fn FooterPlayer() -> Element {
    let CurrentPath(path_sig) = use_context::<CurrentPath>();
    if path_sig() == "/" {
        return rsx! {};
    }

    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let song = state.current_song.read().clone();
    let progress = state.progress.read().clone();
    let player_state = state.player_state.read().clone();
    let volume = *state.volume.read();
    let playing = player_state == PlayerState::PLAYING;

    let title = song.as_ref().and_then(|s| s.title.clone()).unwrap_or_default();
    let artist = song.as_ref().and_then(|s| s.artist.clone()).unwrap_or_default();
    let album = song.as_ref().and_then(|s| s.album.clone()).unwrap_or_default();
    let cur = progress.current_time.as_secs();
    let tot = progress.total_time.as_secs();
    let pct = if tot > 0 {
        (cur as f64 / tot as f64 * 100.0) as u8
    } else {
        0
    };

    rsx! {
        div { class: "player-footer",
            div { class: "w-full bg-base-300 h-0.5",
                div { class: "bg-primary h-0.5", style: "width:{pct}%" }
            }
            div { class: "flex items-center gap-3 px-3 py-3",
                a {
                    class: "flex-1 min-w-0 cursor-pointer",
                    href: "/",
                    onclick: move |e| {
                        e.prevent_default();
                        navigate(path_sig, "/");
                    },
                    p { class: "text-sm font-medium truncate", "{title}" }
                    p { class: "text-xs text-base-content/50 truncate", "{artist}  {album}" }
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "btn btn-ghost btn-xs",
                        onclick: move |_| ws_user_cmd(&ws, UserCommand::Player(PlayerCommand::Prev)),
                        i { class: "material-icons", "skip_previous" }
                    }
                    button {
                        class: "btn btn-primary btn-circle btn-sm",
                        onclick: {
                            
                            move |_| ws_user_cmd(&ws, UserCommand::Player(if playing { PlayerCommand::Pause } else { PlayerCommand::Play }))
                        },
                        i { class: "material-icons", if playing { "pause" } else { "play_arrow" } }
                    }
                    button {
                        class: "btn btn-ghost btn-xs",
                        onclick: move |_| ws_user_cmd(&ws, UserCommand::Player(PlayerCommand::Next)),
                        i { class: "material-icons", "skip_next" }
                    }
                }
                div { class: "hidden sm:flex items-center gap-1 w-24",
                    i { class: "material-icons text-xs text-base-content/50", "volume_down" }
                    input {
                        r#type: "range",
                        class: "range range-xs flex-1",
                        min: volume.min as i64, max: volume.max as i64, value: volume.current as i64,
                        onchange: { let ws = ws; move |e: Event<FormData>| {
                            if let Ok(v) = e.value().parse::<u8>() {
                                send_system_cmd(&ws, SystemCommand::SetVol(v));
                            }
                        }},
                    }
                }
            }
        }
    }
}

// ─── Notifications ──────────────────────────────────────────────────────────

#[component]
fn Notifications() -> Element {
    let state = use_context::<AppState>();
    let connected = *state.connected.read();
    let mut notification = state.notification;
    let mut show_disconnected = use_signal(|| false);

    use_effect(move || {
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3_000).await;
            show_disconnected.set(true);
        });
    });

    // Auto-dismiss notification after 4 seconds
    use_effect(move || {
        let notif = notification.read().clone();
        if notif.is_some() {
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(4_000).await;
                notification.set(None);
            });
        }
    });

    rsx! {
        if !connected && show_disconnected() {
            div { class: "alert alert-error fixed top-0 left-0 right-0 z-50 rounded-none justify-center py-1 transform-gpu",
                i { class: "material-icons mr-2", "wifi_off" }
                span { "Connection lost. Reconnecting..." }
            }
        }
        if let Some(notif) = notification.read().clone() {
            div { class: "fixed top-4 right-4 z-50 max-w-xs transform-gpu",
                match notif {
                    StateChangeEvent::NotificationSuccess(msg) => rsx! {
                        div { class: "alert alert-success shadow-lg", span { "{msg}" } }
                    },
                    StateChangeEvent::NotificationError(msg) => rsx! {
                        div { class: "alert alert-error shadow-lg", span { "{msg}" } }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}
