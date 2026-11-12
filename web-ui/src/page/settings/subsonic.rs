//! Settings section of the Subsonic compatible API (`/rest/`).

use api_models::settings::Settings;
use dioxus::prelude::*;

use super::{SettingsSection, ToggleRow};

/// The address to enter in a Subsonic app: the network address on the
/// desktop app (when it listens on the network), else the one this page
/// was opened at.
fn server_url(settings: &Settings) -> String {
    settings.remote_access_url.clone().unwrap_or_else(|| {
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default()
    })
}

#[component]
pub fn SubsonicSection(settings: Signal<Settings>, on_save: EventHandler<()>) -> Element {
    let s = settings.read().subsonic_settings.clone();
    let url = server_url(&settings.read());
    let local_only = settings.read().desktop_mode && !settings.read().allow_remote_access;
    let demo = settings.read().demo_mode;
    rsx! {
        SettingsSection {
            title: "Subsonic API",
            icon: "phonelink",
            content: rsx! {
                ToggleRow {
                    label: "Enable Subsonic API",
                    checked: s.enabled,
                    onchange: move |_| {
                        let v = !settings.read().subsonic_settings.enabled;
                        settings.write().subsonic_settings.enabled = v;
                        on_save.call(());
                    },
                }
                p { class: "text-xs opacity-60",
                    "Lets Subsonic music apps (Symfonium, DSub, Amperfy, play:Sub, Feishin, Supersonic and others) browse your library and stream its files. Single user, original files only (no transcoding)."
                }
                if s.enabled {
                    div { class: "mt-3 space-y-3",
                        div { class: "form-control",
                            label { class: "label",
                                span { class: "label-text font-medium", "Username" }
                            }
                            input {
                                class: "input input-sm input-bordered w-full",
                                r#type: "text",
                                autocomplete: "off",
                                value: "{s.username}",
                                onchange: move |e: Event<FormData>| {
                                    let name = e.value().trim().to_string();
                                    if !name.is_empty() {
                                        settings.write().subsonic_settings.username = name;
                                        on_save.call(());
                                    }
                                },
                            }
                        }
                        div { class: "form-control",
                            label { class: "label",
                                span { class: "label-text font-medium", "Password" }
                            }
                            input {
                                class: "input input-sm input-bordered w-full",
                                r#type: "password",
                                autocomplete: "new-password",
                                placeholder: "Required",
                                value: "{s.password}",
                                onchange: move |e: Event<FormData>| {
                                    settings.write().subsonic_settings.password = e.value();
                                    on_save.call(());
                                },
                            }
                        }
                        if s.password.is_empty() {
                            p { class: "text-xs text-warning", "Set a password: the API stays off without one." }
                        }
                        if demo {
                            p { class: "text-xs text-warning", "Not available in demo mode." }
                        }
                        div { class: "text-xs opacity-60 space-y-1",
                            p {
                                "Server address for the app: "
                                span { class: "font-mono select-all", "{url}" }
                            }
                            p {
                                "Apps send the password with every request; anyone on your network who sees that traffic can read it. Use a password you don't use elsewhere."
                            }
                        }
                        if local_only {
                            p { class: "text-xs text-warning",
                                "This device only accepts connections from itself. Turn on \"Allow access from other devices\" under Remote Access so phones can connect."
                            }
                        }
                    }
                }
            },
        }
    }
}
