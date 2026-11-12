//! Settings section for scrobbling to ListenBrainz and Last.fm.

use api_models::common::UserCommand;
use api_models::scrobble::ScrobbleCommand;
use api_models::settings::Settings;
use dioxus::prelude::*;
use gloo_net::http::Request;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

const API_SETTINGS_PATH: &str = "/api/settings";

#[component]
pub fn ScrobbleSection(settings: Signal<Settings>, ws: Signal<Option<WebSocket>>, saving: Signal<bool>) -> Element {
    let state = use_context::<AppState>();
    let status = state.scrobble_status;

    use_effect(move || ws_send(&ws, &UserCommand::Scrobble(ScrobbleCommand::QueryStatus)));

    let mut auto_save = move || {
        *saving.write() = true;
        let s = settings.read().clone();
        spawn(async move {
            let _ = Request::post(API_SETTINGS_PATH).json(&s).expect("serialize settings").send().await;
            *saving.write() = false;
        });
    };

    let sc = settings.read().scrobble_settings.clone();
    let st = status.read().clone();
    let lb_queued = st.listenbrainz_queued;
    let lfm_queued = st.lastfm_queued;
    let lb_user = st.listenbrainz_username.clone().unwrap_or_default();
    let lfm_user = st.lastfm_username.clone();
    let auth_url = st.lastfm_auth_url.clone();

    rsx! {
        div { class: "space-y-3",
            p { class: "text-xs opacity-60",
                "Scrobbling sends the artist, title and album of what you play (and when) to the service you turn on here. Radio and podcasts are not scrobbled. A track counts once half of it, or 4 minutes, has been played."
            }

            // ── ListenBrainz ──
            div { class: "space-y-2",
                ToggleRow {
                    label: "ListenBrainz",
                    checked: sc.listenbrainz_enabled,
                    onchange: move |_| {
                        let v = !settings.read().scrobble_settings.listenbrainz_enabled;
                        settings.write().scrobble_settings.listenbrainz_enabled = v;
                        auto_save();
                    },
                }
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-2",
                    div { class: "form-control",
                        label { class: "label py-0.5",
                            span { class: "label-text text-sm", "User token" }
                        }
                        input {
                            class: "input input-sm input-bordered w-full",
                            r#type: "password",
                            autocomplete: "off",
                            value: "{sc.listenbrainz_token}",
                            onchange: move |e: Event<FormData>| {
                                settings.write().scrobble_settings.listenbrainz_token = e.value().trim().to_string();
                                auto_save();
                            },
                        }
                    }
                    div { class: "form-control",
                        label { class: "label py-0.5",
                            span { class: "label-text text-sm", "Server URL" }
                        }
                        input {
                            class: "input input-sm input-bordered w-full",
                            r#type: "text",
                            placeholder: api_models::scrobble::DEFAULT_LISTENBRAINZ_URL,
                            value: "{sc.listenbrainz_url}",
                            onchange: move |e: Event<FormData>| {
                                settings.write().scrobble_settings.listenbrainz_url = e.value().trim().to_string();
                                auto_save();
                            },
                        }
                    }
                }
                div { class: "flex items-center gap-2 flex-wrap",
                    button {
                        class: "btn btn-sm",
                        disabled: sc.listenbrainz_token.is_empty(),
                        onclick: move |_| {
                            let s = settings.read().scrobble_settings.clone();
                            ws_send(
                                &ws,
                                &UserCommand::Scrobble(ScrobbleCommand::TestListenBrainz {
                                    url: s.listenbrainz_url,
                                    token: s.listenbrainz_token,
                                }),
                            );
                        },
                        i { class: "material-icons text-sm", "network_check" }
                        "Test"
                    }
                    if !lb_user.is_empty() {
                        span { class: "text-xs opacity-70", "Token of {lb_user}" }
                    }
                    if lb_queued > 0 {
                        span { class: "text-xs text-warning", "{lb_queued} waiting to be sent" }
                    }
                }
                p { class: "text-xs opacity-60",
                    "Copy your user token from listenbrainz.org/settings. For a self-hosted or compatible server (such as Maloja), enter its API address."
                }
            }

            div { class: "divider my-1" }

            // ── Last.fm ──
            div { class: "space-y-2",
                ToggleRow {
                    label: "Last.fm",
                    checked: sc.lastfm_enabled,
                    onchange: move |_| {
                        let v = !settings.read().scrobble_settings.lastfm_enabled;
                        settings.write().scrobble_settings.lastfm_enabled = v;
                        auto_save();
                    },
                }
                if lfm_user.is_empty() {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-2",
                        div { class: "form-control",
                            label { class: "label py-0.5",
                                span { class: "label-text text-sm", "API key" }
                            }
                            input {
                                class: "input input-sm input-bordered w-full",
                                r#type: "text",
                                autocomplete: "off",
                                value: "{sc.lastfm_api_key}",
                                onchange: move |e: Event<FormData>| {
                                    settings.write().scrobble_settings.lastfm_api_key = e.value().trim().to_string();
                                    auto_save();
                                },
                            }
                        }
                        div { class: "form-control",
                            label { class: "label py-0.5",
                                span { class: "label-text text-sm", "Shared secret" }
                            }
                            input {
                                class: "input input-sm input-bordered w-full",
                                r#type: "password",
                                autocomplete: "off",
                                value: "{sc.lastfm_api_secret}",
                                onchange: move |e: Event<FormData>| {
                                    settings.write().scrobble_settings.lastfm_api_secret = e.value().trim().to_string();
                                    auto_save();
                                },
                            }
                        }
                    }
                    div { class: "flex items-center gap-2 flex-wrap",
                        button {
                            class: "btn btn-sm",
                            disabled: sc.lastfm_api_key.is_empty() || sc.lastfm_api_secret.is_empty(),
                            onclick: move |_| {
                                let s = settings.read().scrobble_settings.clone();
                                ws_send(
                                    &ws,
                                    &UserCommand::Scrobble(ScrobbleCommand::LastFmStartAuth {
                                        api_key: s.lastfm_api_key,
                                        api_secret: s.lastfm_api_secret,
                                    }),
                                );
                            },
                            i { class: "material-icons text-sm", "link" }
                            "Connect"
                        }
                        if let Some(url) = auth_url {
                            a {
                                class: "btn btn-sm btn-primary",
                                href: "{url}",
                                target: "_blank",
                                rel: "noopener",
                                i { class: "material-icons text-sm", "open_in_new" }
                                "Allow access on Last.fm"
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    // Connecting means wanting to scrobble.
                                    settings.write().scrobble_settings.lastfm_enabled = true;
                                    auto_save();
                                    ws_send(&ws, &UserCommand::Scrobble(ScrobbleCommand::LastFmCompleteAuth));
                                },
                                "Finish connecting"
                            }
                        }
                    }
                    p { class: "text-xs opacity-60",
                        "Create an API account at last.fm/api/account/create (any application name, no callback URL needed), enter its API key and shared secret, then press Connect, allow access on the Last.fm page and come back to press Finish connecting."
                    }
                } else {
                    div { class: "flex items-center gap-2 flex-wrap",
                        span { class: "text-sm", "Connected as " }
                        span { class: "text-sm font-medium", "{lfm_user}" }
                        button {
                            class: "btn btn-sm btn-ghost text-error",
                            onclick: move |_| ws_send(&ws, &UserCommand::Scrobble(ScrobbleCommand::LastFmDisconnect)),
                            "Disconnect"
                        }
                    }
                }
                if lfm_queued > 0 {
                    p { class: "text-xs text-warning", "{lfm_queued} waiting to be sent" }
                }
            }
        }
    }
}

#[component]
fn ToggleRow(label: &'static str, checked: bool, onchange: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "flex items-center justify-between py-1.5",
            span { class: "text-sm font-medium", "{label}" }
            input {
                r#type: "checkbox",
                class: "toggle toggle-sm toggle-primary",
                checked,
                onclick: move |e| onchange.call(e),
            }
        }
    }
}
