//! Folder picker modal for Settings → Music Library.
//!
//! Browses the *server's* filesystem one level at a time through
//! `StorageCommand::ListDirectories`; the UI may be a browser on another
//! machine, so a native file dialog would show the wrong computer. Listings
//! are broadcast to every connected client, so the picker only shows the one
//! whose `path` matches the path it last asked for.

use api_models::common::{StorageCommand, UserCommand};
use api_models::state::DirectoryEntry;
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

#[component]
pub fn DirPicker(ws: Signal<Option<WebSocket>>, on_select: EventHandler<String>, on_close: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    // The path whose listing is on screen; empty means the library roots.
    let mut requested = use_signal(String::new);

    use_hook(move || ws_send(&ws, &UserCommand::Storage(StorageCommand::ListDirectories(String::new()))));

    let mut open = move |path: String| {
        requested.set(path.clone());
        ws_send(&ws, &UserCommand::Storage(StorageCommand::ListDirectories(path)));
    };

    let current = requested();
    let at_roots = current.is_empty();
    let listing = state.directory_listing.read().clone().filter(|l| l.path == current);
    let can_select = !at_roots && listing.as_ref().is_some_and(|l| l.error.is_none());
    let up_target = listing.as_ref().and_then(|l| l.parent.clone()).unwrap_or_default();
    let crumbs = listing.as_ref().map(|l| l.breadcrumbs.clone()).unwrap_or_default();
    let selected = current.clone();

    let body = match listing {
        None => rsx! {
            div { class: "flex justify-center p-8",
                span { class: "loading loading-spinner" }
            }
        },
        Some(l) if l.error.is_some() => {
            let error = l.error.unwrap_or_default();
            rsx! {
                div { class: "p-4 text-sm text-error", "Cannot open this folder: {error}" }
            }
        }
        Some(l) if l.entries.is_empty() => rsx! {
            div { class: "p-4 text-sm text-base-content/60",
                if at_roots { "No locations found — type a path instead." } else { "No sub-folders here." }
            }
        },
        Some(l) => {
            let truncated = l.truncated;
            let shown = l.entries.len();
            rsx! {
                {l.entries.into_iter().map(|entry| {
                    let key = entry.path.clone();
                    rsx! {
                        DirRow { key: "{key}", entry, on_open: move |path| open(path) }
                    }
                })}
                if truncated {
                    p { class: "px-4 py-2 text-xs text-base-content/60", "Only the first {shown} folders are shown." }
                }
            }
        }
    };

    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-box w-full max-w-lg p-0 flex flex-col h-[100dvh] max-h-[100dvh] rounded-none sm:h-auto sm:max-h-[80vh] sm:rounded-box",
                div { class: "flex items-center gap-1 px-4 pt-4 pb-2",
                    if !at_roots {
                        button {
                            class: "btn btn-ghost btn-sm btn-square",
                            title: "Up",
                            onclick: move |_| open(up_target.clone()),
                            i { class: "material-icons", "arrow_upward" }
                        }
                    }
                    h3 { class: "font-bold text-lg flex-1", "Choose music folder" }
                    button {
                        class: "btn btn-ghost btn-sm btn-square",
                        title: "Close",
                        onclick: move |_| on_close.call(()),
                        i { class: "material-icons", "close" }
                    }
                }
                div { class: "px-4 pb-2 flex flex-wrap items-center gap-x-1 gap-y-1 text-sm",
                    button { class: "link link-hover", onclick: move |_| open(String::new()), "Locations" }
                    {crumbs.into_iter().map(|crumb| {
                        let path = crumb.path.clone();
                        rsx! {
                            span { key: "{crumb.path}", class: "flex items-center gap-x-1 min-w-0",
                                i { class: "material-icons text-sm text-base-content/40", "chevron_right" }
                                button {
                                    class: "link link-hover truncate max-w-[12rem]",
                                    onclick: move |_| open(path.clone()),
                                    "{crumb.name}"
                                }
                            }
                        }
                    })}
                }
                div { class: "flex-1 min-h-0 overflow-y-auto border-y border-base-300", {body} }
                div { class: "p-4 flex flex-col gap-2 sm:flex-row sm:items-center",
                    p { class: "text-xs text-base-content/60 truncate flex-1",
                        if at_roots { "Pick a location to start." } else { "{current}" }
                    }
                    div { class: "flex gap-2 justify-end",
                        button { class: "btn btn-sm", onclick: move |_| on_close.call(()), "Cancel" }
                        button {
                            class: "btn btn-sm btn-primary",
                            disabled: !can_select,
                            onclick: move |_| on_select.call(selected.clone()),
                            "Use this folder"
                        }
                    }
                }
            }
            div { class: "modal-backdrop", onclick: move |_| on_close.call(()) }
        }
    }
}

#[component]
fn DirRow(entry: DirectoryEntry, on_open: EventHandler<String>) -> Element {
    let icon = if entry.label.is_some() {
        "storage"
    } else if entry.audio_files > 0 {
        "library_music"
    } else {
        "folder"
    };
    let detail = if !entry.readable {
        "No access".to_string()
    } else if entry.label.is_some() {
        entry.path.clone()
    } else {
        summary(entry.subdirs, entry.audio_files)
    };
    let path = entry.path.clone();
    rsx! {
        button {
            class: "w-full flex items-center gap-3 px-4 min-h-12 py-2 text-left hover:bg-base-200 disabled:opacity-50",
            disabled: !entry.readable,
            onclick: move |_| on_open.call(path.clone()),
            i { class: "material-icons text-base-content/60", "{icon}" }
            div { class: "flex-1 min-w-0",
                p { class: "text-sm truncate", "{entry.name}" }
                p { class: "text-xs text-base-content/50 truncate", "{detail}" }
            }
            i { class: "material-icons text-base-content/30", "chevron_right" }
        }
    }
}

fn summary(subdirs: u32, audio_files: u32) -> String {
    let folders = match subdirs {
        0 => None,
        1 => Some("1 folder".to_string()),
        n => Some(format!("{n} folders")),
    };
    let files = match audio_files {
        0 => None,
        1 => Some("1 audio file".to_string()),
        n => Some(format!("{n} audio files")),
    };
    match (folders, files) {
        (None, None) => "No music".to_string(),
        (Some(one), None) | (None, Some(one)) => one,
        (Some(folders), Some(files)) => format!("{folders} · {files}"),
    }
}
