//! Sleep timer controls: the player page button with its menu, and the
//! small indicator in the mini player.
//!
//! The timer runs on the server; `AppState::sleep_timer` mirrors its state
//! from `SleepTimerEvent`. The countdown is computed here from `ends_at`,
//! and the label signal only changes when the text it shows changes (once a
//! second under an hour, once a minute above), so nothing else re-renders.

use api_models::{
    common::{PlayerCommand, UserCommand},
    sleep_timer::{SleepTimerRequest, SleepTimerState, SLEEP_TIMER_MAX_MINUTES},
};
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState, UiState};

const PRESET_MINUTES: [u16; 6] = [15, 30, 45, 60, 90, 120];

/// "12:05" below an hour, "1 h 12 min" above.
fn format_remaining(secs: u64) -> String {
    if secs >= 3600 {
        let (h, m) = (secs / 3600, (secs % 3600) / 60);
        if m == 0 {
            format!("{h} h")
        } else {
            format!("{h} h {m} min")
        }
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

fn label_for(state: SleepTimerState) -> Option<String> {
    match state {
        SleepTimerState::Off => None,
        SleepTimerState::EndOfTrack => Some("End of track".to_string()),
        SleepTimerState::Until { ends_at } => {
            let left_ms = ends_at.timestamp_millis() as f64 - js_sys::Date::now();
            let secs = (left_ms / 1000.0).ceil().max(0.0) as u64;
            Some(format_remaining(secs))
        }
    }
}

/// The remaining time (or "End of track"), `None` while no timer runs.
fn use_sleep_timer_label() -> Signal<Option<String>> {
    let state = use_context::<AppState>();
    let timer = state.sleep_timer;
    let page_visible = state.page_visible;
    let mut label = use_signal(|| label_for(*timer.peek()));
    use_effect(move || {
        let current = label_for(*timer.read());
        if *label.peek() != current {
            label.set(current);
        }
    });
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            let current = *timer.peek();
            if matches!(current, SleepTimerState::Until { .. }) && *page_visible.peek() {
                let text = label_for(current);
                if *label.peek() != text {
                    label.set(text);
                }
            }
        }
    });
    label
}

/// What runs after the pause (Settings → Sleep timer), as far as this
/// server offers it; `None` when it only pauses.
fn then_line(settings: &api_models::settings::Settings) -> Option<String> {
    let wanted = settings.sleep_timer_settings;
    let firmware = wanted.firmware_power_off && settings.firmware_power_available();
    let shutdown = wanted.shutdown_system && settings.system_power_available();
    match (firmware, shutdown) {
        (true, true) => Some("Then: power off and shut down the system".to_string()),
        (true, false) => Some("Then: power off (firmware control channel)".to_string()),
        (false, true) => Some("Then: shut down the system".to_string()),
        (false, false) => None,
    }
}

fn set_timer(ws: &Signal<Option<WebSocket>>, request: SleepTimerRequest) {
    ws_send(ws, &UserCommand::Player(PlayerCommand::SetSleepTimer(request)));
}

/// Player page button: shows the remaining time and opens the timer menu
/// (rendered by the player page, see [`SleepTimerMenu`]).
#[component]
pub fn SleepTimerButton() -> Element {
    let state = use_context::<AppState>();
    let mut ui = use_context::<UiState>();
    let label = use_sleep_timer_label();
    let active = state.sleep_timer.read().is_active();
    let title = label.read().as_ref().map_or_else(|| "Sleep timer".to_string(), |l| format!("Sleep timer: {l}"));
    rsx! {
        button {
            class: if active { "btn btn-ghost btn-sm text-primary gap-1 px-2" } else { "btn btn-ghost btn-sm" },
            title: "{title}",
            onclick: move |_| ui.sleep_timer_open.set(true),
            i { class: "material-icons", "bedtime" }
            if let Some(text) = label.read().as_ref() {
                span { class: "text-xs tabular-nums", "{text}" }
            }
        }
    }
}

/// The timer choices. "End of track" is not offered for live radio.
#[component]
pub fn SleepTimerMenu(ws: Signal<Option<WebSocket>>) -> Element {
    let state = use_context::<AppState>();
    let ui = use_context::<UiState>();
    let label = use_sleep_timer_label();
    let mut custom = use_signal(String::new);
    let custom_minutes = custom.read().trim().parse::<u16>().ok().filter(|m| (1..=SLEEP_TIMER_MAX_MINUTES).contains(m));
    let timer = *state.sleep_timer.read();
    let active = timer.is_active();
    let end_of_track = !state.playback_source.read().is_radio();
    let status = match (timer, label.read().as_ref()) {
        (SleepTimerState::EndOfTrack, _) => "Pausing at the end of this track".to_string(),
        (SleepTimerState::Until { .. }, Some(l)) => format!("Pausing in {l}"),
        _ => "Pause playback after:".to_string(),
    };
    // Fetched when the menu opens, so a change made on another device shows.
    let then = use_resource(|| async {
        let settings: api_models::settings::Settings = gloo_net::http::Request::get("/api/settings").send().await.ok()?.json().await.ok()?;
        then_line(&settings)
    });
    let on_close = move || {
        let mut open = ui.sleep_timer_open;
        open.set(false);
    };
    let choose = move |request: SleepTimerRequest| {
        set_timer(&ws, request);
        on_close();
    };
    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-backdrop", onclick: move |_| on_close() }
            div { class: "modal-box max-w-sm bg-base-300",
                button {
                    class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    title: "Close",
                    onclick: move |_| on_close(),
                    i { class: "material-icons text-base", "close" }
                }
                h3 { class: "font-bold text-lg flex items-center gap-2 mb-1",
                    i { class: "material-icons", "bedtime" }
                    "Sleep timer"
                }
                p { class: "text-sm text-base-content/60", "{status}" }
                if let Some(Some(line)) = then.read().as_ref() {
                    p { class: "text-sm text-warning flex items-center gap-1 mt-1",
                        i { class: "material-icons text-base", "power_settings_new" }
                        "{line}"
                    }
                }
                div { class: "mb-4" }
                div { class: "grid grid-cols-3 gap-2",
                    for minutes in PRESET_MINUTES {
                        button {
                            key: "{minutes}",
                            class: "btn btn-sm btn-outline",
                            onclick: move |_| choose(SleepTimerRequest::Minutes(minutes)),
                            "{minutes} min"
                        }
                    }
                }
                if end_of_track {
                    button {
                        class: "btn btn-sm btn-outline w-full mt-2",
                        onclick: move |_| choose(SleepTimerRequest::EndOfTrack),
                        i { class: "material-icons text-base", "last_page" }
                        "End of track"
                    }
                }
                form {
                    class: "flex gap-2 mt-2",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if let Some(m) = custom_minutes {
                            choose(SleepTimerRequest::Minutes(m));
                        }
                    },
                    input {
                        r#type: "number",
                        class: "input input-sm input-bordered flex-1 min-w-0",
                        min: "1",
                        max: "{SLEEP_TIMER_MAX_MINUTES}",
                        placeholder: "Minutes",
                        aria_label: "Custom minutes",
                        value: "{custom}",
                        oninput: move |e| custom.set(e.value()),
                    }
                    button {
                        r#type: "submit",
                        class: "btn btn-sm btn-primary",
                        disabled: custom_minutes.is_none(),
                        "Set"
                    }
                }
                if active {
                    button {
                        class: "btn btn-sm btn-outline btn-error w-full mt-4",
                        onclick: move |_| choose(SleepTimerRequest::Off),
                        i { class: "material-icons text-base", "timer_off" }
                        "Turn off"
                    }
                }
            }
        }
    }
}

/// Mini player indicator, shown while a timer runs.
#[component]
pub fn SleepTimerBadge() -> Element {
    let label = use_sleep_timer_label();
    let Some(text) = label.read().clone() else {
        return rsx! {};
    };
    rsx! {
        span {
            class: "badge badge-sm badge-ghost gap-1 shrink-0 text-primary tabular-nums",
            title: "Sleep timer: {text}",
            i { class: "material-icons text-sm", "bedtime" }
            span { class: "hidden sm:inline", "{text}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_remaining;

    #[test]
    fn formats_remaining_time() {
        assert_eq!(format_remaining(0), "0:00");
        assert_eq!(format_remaining(65), "1:05");
        assert_eq!(format_remaining(59 * 60 + 59), "59:59");
        assert_eq!(format_remaining(3600), "1 h");
        assert_eq!(format_remaining(3600 + 12 * 60 + 30), "1 h 12 min");
    }
}
