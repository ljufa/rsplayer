use crate::state::AppState;
use api_models::{
    common::{PlayerCommand, QueueCommand, SystemCommand, UserCommand},
    state::StateChangeEvent,
};
use dioxus::prelude::*;
use gloo_console::log;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

/// Manage the WebSocket connection.  Returns a signal for the sender function.
/// Auto-reconnects with a simple 3-second delay on close/error.
pub fn use_websocket(mut app_state: AppState) -> Signal<Option<WebSocket>> {
    let ws_signal: Signal<Option<WebSocket>> = use_signal(|| None);
    let ws_ref = use_signal(|| ws_signal);

    use_effect(move || {
        connect(app_state.clone(), ws_ref);
    });

    ws_signal
}

fn connect(mut app_state: AppState, ws_holder: Signal<Signal<Option<WebSocket>>>) {
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
        let mut state = app_state.clone();
        let mut holder = ws_holder;
        let ws_clone = ws.clone();
        let onopen = Closure::<dyn FnMut(JsValue)>::new(move |_| {
            *state.connected.write() = true;
            *holder.write().write() = Some(ws_clone.clone());
            log!("WebSocket connected");
            // Request initial state from the backend
            let send = |cmd: &str| {
                let _ = ws_clone.send_with_str(cmd);
            };
            if let Ok(json) = serde_json::to_string(&UserCommand::Queue(QueueCommand::QueryCurrentSong)) {
                send(&json);
            }
            if let Ok(json) = serde_json::to_string(&UserCommand::Player(PlayerCommand::QueryCurrentPlayerInfo)) {
                send(&json);
            }
            if let Ok(json) = serde_json::to_string(&SystemCommand::QueryCurrentVolume) {
                send(&json);
            }
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();
    }

    // onclose
    {
        let state = app_state.clone();
        let onclose = Closure::<dyn FnMut(CloseEvent)>::new(move |_e: CloseEvent| {
            let mut s = state.clone();
            *s.connected.write() = false;
            log!("WebSocket closed — reconnecting in 3s");
            let s2 = state.clone();
            let holder = ws_holder;
            wasm_bindgen_futures::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(3_000).await;
                connect(s2, holder);
            });
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();
    }

    // onerror
    {
        let onerror = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
            gloo_console::error!("WebSocket error:", e.message());
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
    }
}

fn schedule_reconnect(app_state: AppState, ws_holder: Signal<Signal<Option<WebSocket>>>) {
    wasm_bindgen_futures::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(3_000).await;
        connect(app_state, ws_holder);
    });
}

/// Send a user command over the websocket.
pub fn ws_send(ws: &Signal<Option<WebSocket>>, cmd: &UserCommand) {
    if let Some(ws) = ws.read().as_ref() {
        if let Ok(json) = serde_json::to_string(cmd) {
            let _ = ws.send_with_str(&json);
        }
    }
}
