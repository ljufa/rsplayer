use api_models::{
    common::{dur_to_string, PlayerCommand, QueueCommand, UserCommand},
    player::Song,
    state::CurrentQueueQuery,
};
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState, UiState};

#[component]
pub fn QueuePage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let mut ui = use_context::<UiState>();
    let current_song_id = state.current_song.read().as_ref().map(|s| s.file.clone());

    let queue = state.current_queue;
    let mut loading = use_signal(|| true);
    let mut search = use_signal(String::new);
    let mut dragged_idx: Signal<Option<usize>> = use_signal(|| None);

    // Initial load
    use_effect(move || {
        ws_send(
            &ws,
            &UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                String::new(),
                0,
            ))),
        );
    });

    // Clear loading when queue data arrives
    use_effect(move || {
        if queue.read().is_some() {
            *loading.write() = false;
        }
    });

    let mut do_search = move || {
        *loading.write() = true;
        ws_send(
            &ws,
            &UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                search(),
                0,
            ))),
        );
    };

    let load_more = move |offset: usize| {
        ws_send(
            &ws,
            &UserCommand::Queue(QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                search(),
                offset,
            ))),
        );
    };

    rsx! {
        div { class: "queue-page",

            // ── Toolbar ─────────────────────────────────────────────────────
            div { class: "flex flex-col gap-1 px-3 py-2 border-b border-base-300",
                // Row 1: search field + search/clear buttons
                div { class: "flex items-center gap-2",
                    input {
                        class: "input input-sm input-bordered flex-1",
                        r#type: "text",
                        placeholder: "Find a song",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter { do_search(); }
                        },
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Search",
                        onclick: move |_| do_search(),
                        i { class: "material-icons text-base", "search" }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Clear search",
                        onclick: move |_| {
                            search.set(String::new());
                            *loading.write() = true;
                            ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                CurrentQueueQuery::WithSearchTerm(String::new(), 0)
                            )));
                        },
                        i { class: "material-icons text-base", "backspace" }
                    }
                }
                // Row 2: queue action buttons
                div { class: "flex items-center gap-1",
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Show from current song",
                        onclick: move |_| {
                            *loading.write() = true;
                            ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                CurrentQueueQuery::CurrentSongPage
                            )));
                        },
                        i { class: "material-icons text-base", "filter_center_focus" }
                        span { class: "hidden sm:inline text-xs", "Focus" }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Add URL to queue",
                        onclick: move |_| { ui.queue_add_url_input.set(String::new()); ui.queue_add_url_open.set(true); },
                        i { class: "material-icons text-base", "queue" }
                        span { class: "hidden sm:inline text-xs", "Add URL" }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Save queue as playlist",
                        onclick: move |_| ui.queue_save_playlist_open.set(true),
                        i { class: "material-icons text-base", "save" }
                        span { class: "hidden sm:inline text-xs", "Save" }
                    }
                    div { class: "flex-1" }
                    button {
                        class: "btn btn-sm btn-ghost text-error",
                        title: "Clear queue",
                        onclick: move |_| ui.queue_clear_confirm_open.set(true),
                        i { class: "material-icons text-base", "clear" }
                        span { class: "hidden sm:inline text-xs", "Clear" }
                    }
                }
            }

            // ── Queue content ───────────────────────────────────────────────
            if loading() && queue().is_none() {
                QueueSkeleton {}
            } else if queue().as_ref().is_none_or(|p| p.items.is_empty()) {
                QueueEmpty {
                    has_search: !search().is_empty(),
                    on_clear_search: move |_| {
                        search.set(String::new());
                        *loading.write() = true;
                        ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                            CurrentQueueQuery::WithSearchTerm(String::new(), 0)
                        )));
                    },
                    on_add_url: move |_| ui.queue_add_url_open.set(true),
                }
            } else if let Some(page) = queue() {
                div { class: "scroll-list overflow-y-auto",
                    {page.items.iter().enumerate().map(|(idx, song)| {
                        let song = song.clone();
                        let is_current = current_song_id.as_ref().is_some_and(|id| *id == song.file);
                        let file = song.file.clone();
                        let file2 = song.file.clone();
                        let _file3 = song.file.clone();
                        let start_idx = page.offset.saturating_sub(page.limit);
                        let abs_idx = start_idx + idx;
                        rsx! {
                            QueueItem {
                                key: "{song.file}",
                                song,
                                idx: abs_idx,
                                is_current,
                                on_play: move |_| {
                                    ws_send(&ws, &UserCommand::Player(PlayerCommand::PlayItem(file.clone())));
                                },
                                on_play_next: move |_| {
                                    ws_send(&ws, &UserCommand::Queue(QueueCommand::MoveItemAfterCurrent(abs_idx)));
                                    ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                        CurrentQueueQuery::WithSearchTerm(search(), 0)
                                    )));
                                },
                                on_remove: move |_| {
                                    ws_send(&ws, &UserCommand::Queue(QueueCommand::RemoveItem(file2.clone())));
                                    let mut cq = state.current_queue;
                                    let mut guard = cq.write();
                                    if let Some(q) = guard.as_mut() {
                                        q.remove_item(&file2);
                                    }
                                },
                                on_drag_start: move |_| dragged_idx.set(Some(abs_idx)),
                                on_drop: move |_| {
                                    if let Some(from) = dragged_idx() {
                                        if from != abs_idx {
                                            ws_send(&ws, &UserCommand::Queue(QueueCommand::MoveItem(from, abs_idx)));
                                            ws_send(&ws, &UserCommand::Queue(QueueCommand::QueryCurrentQueue(
                                                CurrentQueueQuery::WithSearchTerm(search(), 0)
                                            )));
                                        }
                                    }
                                    dragged_idx.set(None);
                                },
                            }
                        }
                    })}
                    button {
                        class: "btn btn-outline btn-primary btn-sm w-full mt-2",
                        onclick: move |_| load_more(page.offset),
                        "Load more"
                    }
                }
            }
        }
    }
}

#[component]
fn QueueItem(
    song: Song,
    idx: usize,
    is_current: bool,
    on_play: EventHandler,
    on_play_next: EventHandler,
    on_remove: EventHandler,
    on_drag_start: EventHandler,
    on_drop: EventHandler,
) -> Element {
    let title = song.get_title();
    let artist = song.artist.clone().unwrap_or_default();
    let date = song.date.clone().map(|d| format!(" • {d}")).unwrap_or_default();
    let duration = song
        .time
        .as_ref()
        .map(|t| format!(" • {}", dur_to_string(t)))
        .unwrap_or_default();
    let row_class = if is_current {
        "flex items-center gap-1 px-2 py-1.5 bg-primary/10 border-l-2 border-primary"
    } else {
        "flex items-center gap-1 px-2 py-1.5 border-b border-base-200 hover:bg-base-200/50"
    };
    rsx! {
        div {
            class: row_class,
            id: if is_current { "current" } else { "" },
            draggable: true,
            ondragstart: move |_| on_drag_start.call(()),
            ondragover: move |e| e.prevent_default(),
            ondrop: move |e| { e.prevent_default(); on_drop.call(()); },
            // drag handle
            span { class: "queue-item__drag text-base-content/30 cursor-grab px-2",
                i { class: "material-icons text-base", "drag_handle" }
            }
            // track info
            div {
                class: "queue-item__info flex-1 min-w-0 cursor-pointer",
                onclick: move |_| on_play.call(()),
                p { class: "font-medium truncate text-sm", "{title}" }
                p { class: "text-xs text-base-content/50 truncate", "{artist}{date}{duration}" }
            }
            // action buttons
            div { class: "queue-item__actions flex items-center gap-1",
                button {
                    class: "btn btn-ghost btn-xs",
                    title: "Play next",
                    onclick: move |_| on_play_next.call(()),
                    i { class: "material-icons text-sm", "playlist_play" }
                }
                button {
                    class: "btn btn-ghost btn-xs",
                    title: "Play",
                    onclick: move |_| on_play.call(()),
                    i { class: "material-icons text-sm", "play_arrow" }
                }
                button {
                    class: "btn btn-ghost btn-xs text-error",
                    title: "Remove",
                    onclick: move |_| on_remove.call(()),
                    i { class: "material-icons text-sm", "delete" }
                }
            }
        }
    }
}

#[component]
fn QueueEmpty(has_search: bool, on_clear_search: EventHandler, on_add_url: EventHandler) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-16 gap-4 text-base-content/40",
            i { class: "material-icons text-5xl",
                if has_search { "search_off" } else { "queue_music" }
            }
            p { class: "text-lg font-medium",
                if has_search { "No matching songs" } else { "Queue is empty" }
            }
            p { class: "text-sm text-center max-w-xs",
                if has_search {
                    "No songs match your search. Try a different term."
                } else {
                    "Add songs from your library to start listening."
                }
            }
            if has_search {
                button {
                    class: "btn btn-outline btn-sm",
                    onclick: move |_| on_clear_search.call(()),
                    i { class: "material-icons text-sm mr-1", "backspace" }
                    "Clear Search"
                }
            } else {
                button {
                    class: "btn btn-outline btn-sm",
                    onclick: move |_| on_add_url.call(()),
                    i { class: "material-icons text-sm mr-1", "add" }
                    "Add URL"
                }
            }
        }
    }
}

#[component]
fn QueueSkeleton() -> Element {
    rsx! {
        div { class: "flex flex-col gap-2 p-3",
            {(0..8).map(|_| rsx! {
                div { class: "flex items-center gap-3 p-2",
                    div { class: "skeleton w-4 h-4 rounded" }
                    div { class: "flex-1 flex flex-col gap-1",
                        div { class: "skeleton h-4 w-3/4 rounded" }
                        div { class: "skeleton h-3 w-1/2 rounded" }
                    }
                }
            })}
        }
    }
}
