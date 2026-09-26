use crate::state::AppState;
use api_models::{
    common::{MetadataCommand, MultiroomCommand, PlayerCommand, QueueCommand, SystemRequest, UserCommand},
    state::StateChangeEvent,
};
use dioxus::prelude::*;
use gloo_console::log;
use std::cell::{Cell, RefCell};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket};

/// Delay before each reconnect attempt; the last one repeats. The first
/// retry is immediate so a socket dropped while the page was frozen (Android
/// app in the background) comes back before anyone notices.
const RECONNECT_DELAYS_MS: [u32; 5] = [0, 500, 1_000, 2_000, 3_000];

thread_local! {
    /// Bumped by every `connect`: events from superseded sockets and retries
    /// scheduled before a newer connect are ignored.
    static GENERATION: Cell<u32> = const { Cell::new(0) };
    /// Consecutive failed attempts, indexes `RECONNECT_DELAYS_MS`.
    static FAILED_ATTEMPTS: Cell<u32> = const { Cell::new(0) };
    static LISTENERS_INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// The newest socket, including while it is still connecting (the
    /// returned signal only gets it once open — consumers re-query then).
    static CURRENT_SOCKET: RefCell<Option<WebSocket>> = const { RefCell::new(None) };
}

/// Manage the WebSocket connection.  Returns a signal for the sender function.
/// Auto-reconnects with a short backoff on close, and immediately when the
/// page becomes visible again or the network comes back.
pub fn use_websocket(app_state: AppState) -> Signal<Option<WebSocket>> {
    let ws_signal: Signal<Option<WebSocket>> = use_signal(|| None);
    let ws_ref = use_signal(|| ws_signal);

    use_effect(move || {
        install_wake_listeners(app_state.clone(), ws_ref);
        connect(app_state.clone(), ws_ref);
    });

    ws_signal
}

fn connect(app_state: AppState, ws_holder: Signal<Signal<Option<WebSocket>>>) {
    let generation = GENERATION.with(|g| {
        let next = g.get().wrapping_add(1);
        g.set(next);
        next
    });

    let window = web_sys::window().expect("no window");
    let host = window.location().host().unwrap_or_else(|_| "localhost".to_string());
    let protocol = window.location().protocol().unwrap_or_default();
    let ws_scheme = if protocol == "https:" { "wss" } else { "ws" };
    let url = format!("{ws_scheme}://{host}/api/ws");

    let ws = match WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(e) => {
            gloo_console::error!("WebSocket creation failed:", e);
            schedule_reconnect(app_state, ws_holder);
            return;
        }
    };
    // Lets the wake listeners see a socket that is still connecting, so they
    // don't open a second one.
    CURRENT_SOCKET.with(|c| *c.borrow_mut() = Some(ws.clone()));

    // onmessage
    {
        let mut state = app_state.clone();
        let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Some(text) = e.data().as_string() {
                match serde_json::from_str::<StateChangeEvent>(&text) {
                    Ok(event) => state.dispatch(event),
                    Err(err) => gloo_console::error!("WS parse error:", err.to_string()),
                }
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    }

    // onopen
    {
        // Not redundant: `app_state` is used again in the onclose closure below.
        #[allow(clippy::redundant_clone)]
        let mut state = app_state.clone();
        let mut holder = ws_holder;
        let ws_clone = ws.clone();
        let onopen = Closure::<dyn FnMut(JsValue)>::new(move |_| {
            if !is_current(generation) {
                ws_clone.close().ok();
                return;
            }
            FAILED_ATTEMPTS.with(|a| a.set(0));
            if !*state.connected.peek() {
                *state.connected.write() = true;
            }
            *holder.write().write() = Some(ws_clone.clone());
            log!("WebSocket connected");
            request_current_state(&ws_clone);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();
    }

    // onclose
    {
        let state = app_state;
        let onclose = Closure::<dyn FnMut(CloseEvent)>::new(move |_e: CloseEvent| {
            if !is_current(generation) {
                return;
            }
            // Only on the transition: every write notifies subscribers, and
            // retries close again every few seconds while the server is down.
            let mut s = state.clone();
            if *s.connected.peek() {
                *s.connected.write() = false;
            }
            log!("WebSocket closed — reconnecting");
            schedule_reconnect(state.clone(), ws_holder);
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();
    }

    // onerror
    {
        // A WebSocket `error` is a plain Event (no message); details are in
        // the browser console and the following `close`.
        let onerror = Closure::<dyn FnMut(Event)>::new(move |_e: Event| {
            gloo_console::error!("WebSocket error");
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
    }
}

fn is_current(generation: u32) -> bool {
    GENERATION.with(Cell::get) == generation
}

fn schedule_reconnect(app_state: AppState, ws_holder: Signal<Signal<Option<WebSocket>>>) {
    let attempt = FAILED_ATTEMPTS.with(|a| {
        let n = a.get();
        a.set(n.saturating_add(1));
        n as usize
    });
    let delay = RECONNECT_DELAYS_MS[attempt.min(RECONNECT_DELAYS_MS.len() - 1)];
    let generation = GENERATION.with(Cell::get);
    wasm_bindgen_futures::spawn_local(async move {
        if delay > 0 {
            gloo_timers::future::TimeoutFuture::new(delay).await;
        }
        // A wake listener may have reconnected in the meantime.
        if is_current(generation) {
            connect(app_state, ws_holder);
        }
    });
}

/// Ask the backend for everything the player screen shows.
fn request_current_state(ws: &WebSocket) {
    let commands = [
        UserCommand::Queue(QueueCommand::QueryCurrentSong),
        UserCommand::Player(PlayerCommand::QueryCurrentPlayerInfo),
        UserCommand::Player(PlayerCommand::QueryPlaybackSource),
        // The heart button on a playing station shows whether it is a favorite.
        UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations),
        UserCommand::System(SystemRequest::QueryCurrentVolume),
        UserCommand::Multiroom(MultiroomCommand::QueryState),
    ];
    for cmd in &commands {
        if let Ok(json) = serde_json::to_string(cmd) {
            let _ = ws.send_with_str(&json);
        }
    }
}

/// Reconnect right away when the page comes back (tab shown, Android app
/// resumed) or the network returns, instead of waiting out the backoff.
fn install_wake_listeners(app_state: AppState, ws_holder: Signal<Signal<Option<WebSocket>>>) {
    if LISTENERS_INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let on_wake = Closure::<dyn FnMut()>::new(move || {
        let hidden = web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.hidden());
        if hidden {
            return;
        }
        let current = CURRENT_SOCKET.with(|c| c.borrow().clone());
        match current {
            Some(ws) if ws.ready_state() == WebSocket::OPEN => {
                // Events may have been missed while the page was frozen.
                request_current_state(&ws);
            }
            Some(ws) if ws.ready_state() == WebSocket::CONNECTING => {}
            _ => {
                FAILED_ATTEMPTS.with(|a| a.set(0));
                connect(app_state.clone(), ws_holder);
            }
        }
    });
    if let Some(document) = window.document() {
        let _ = document.add_event_listener_with_callback("visibilitychange", on_wake.as_ref().unchecked_ref());
    }
    let _ = window.add_event_listener_with_callback("online", on_wake.as_ref().unchecked_ref());
    on_wake.forget();
}

/// Send a user command over the websocket.
pub fn ws_send(ws: &Signal<Option<WebSocket>>, cmd: &UserCommand) {
    if let Some(ws) = ws.read().as_ref() {
        if let Ok(json) = serde_json::to_string(cmd) {
            let _ = ws.send_with_str(&json);
        }
    }
}
