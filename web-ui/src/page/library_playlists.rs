use std::collections::HashSet;

use api_models::{
    common::{PlaylistCommand, QueueCommand, UserCommand},
    playlist::{Album, PlaylistType},
};
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState, UiState};

fn pl_slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

#[component]
pub fn LibraryPlaylistsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let mut ui = use_context::<UiState>();

    let playlists = state.playlists;
    let lazy_genre_albums = state.lazy_genre_albums;
    let lazy_decade_albums = state.lazy_decade_albums;

    let default_expanded: HashSet<String> = ["recently-added-pl", "new-releases-pl", "saved-pl", "favorites-pl"]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    let mut expanded: Signal<HashSet<String>> = use_signal(|| default_expanded);

    use_effect(move || {
        ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryPlaylist));
    });

    rsx! {
        div { class: "library-page",

            // ── Loading skeleton ─────────────────────────────────────────
            if playlists.read().is_none() {
                div { class: "flex flex-col gap-6 p-3",
                    {(0..3).map(|i| rsx! {
                         div { key: "{i}", class: "flex flex-col gap-2",
                            div { class: "skeleton h-8 w-48 rounded" }
                            div { class: "flex gap-3",
                                {(0..4).map(|j| rsx! { div { key: "{j}", class: "skeleton w-32 h-40 rounded-lg" } })}
                            }
                        }
                    })}
                }
            } else {
                div { class: "overflow-y-auto pb-20",
                    {
                        let pl = playlists.read();
                        let items = pl.as_ref().map(|p| p.items.clone()).unwrap_or_default();

                        let recently_added: Vec<_> = items.iter().filter(|i| i.is_recently_added()).cloned().collect();
                        let new_releases:   Vec<_> = items.iter().filter(|i| i.is_new_release()).cloned().collect();
                        let saved:          Vec<_> = items.iter().filter(|i| i.is_saved()).cloned().collect();
                        let favorites:      Vec<_> = items.iter().filter(|i| i.is_most_played() || i.is_liked()).cloned().collect();
                        let genre_headers   = pl.as_ref().map(api_models::playlist::Playlists::genre_headers).unwrap_or_default();
                        let decade_headers  = pl.as_ref().map(api_models::playlist::Playlists::decade_headers).unwrap_or_default();

                        rsx! {
                            if !recently_added.is_empty() {
                                SectionHeader {
                                    title: "Recently Added", count: recently_added.len(),
                                    is_expanded: expanded.read().contains("recently-added-pl"),
                                    on_toggle: move |()| { let id = "recently-added-pl".to_string(); let mut e = expanded.write(); if e.contains(&id) { e.remove(&id); } else { e.insert(id); } },
                                }
                                if expanded.read().contains("recently-added-pl") {
                                    AlbumCarousel {
                                        row_id: "row-recently-added",
                                        items: recently_added,
                                        on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(true); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumItems(id, 0))); },
                                        on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadAlbumInQueue(id))),
                                        on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddAlbumToQueue(id))),
                                    }
                                }
                            }

                            if !new_releases.is_empty() {
                                SectionHeader {
                                    title: "New Releases", count: new_releases.len(),
                                    is_expanded: expanded.read().contains("new-releases-pl"),
                                    on_toggle: move |()| { let id = "new-releases-pl".to_string(); let mut e = expanded.write(); if e.contains(&id) { e.remove(&id); } else { e.insert(id); } },
                                }
                                if expanded.read().contains("new-releases-pl") {
                                    AlbumCarousel {
                                        row_id: "row-new-releases",
                                        items: new_releases,
                                        on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(true); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumItems(id, 0))); },
                                        on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadAlbumInQueue(id))),
                                        on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddAlbumToQueue(id))),
                                    }
                                }
                            }

                            if !saved.is_empty() {
                                SectionHeader {
                                    title: "Saved Playlists", count: saved.len(),
                                    is_expanded: expanded.read().contains("saved-pl"),
                                    on_toggle: move |()| { let id = "saved-pl".to_string(); let mut e = expanded.write(); if e.contains(&id) { e.remove(&id); } else { e.insert(id); } },
                                }
                                if expanded.read().contains("saved-pl") {
                                    PlaylistCarousel {
                                        row_id: "row-saved",
                                        items: saved,
                                        on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(false); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryPlaylistItems(id, 0))); },
                                        on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadPlaylistInQueue(id))),
                                        on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddPlaylistToQueue(id))),
                                    }
                                }
                            }

                            if !favorites.is_empty() {
                                SectionHeader {
                                    title: "Favorites", count: favorites.len(),
                                    is_expanded: expanded.read().contains("favorites-pl"),
                                    on_toggle: move |()| { let id = "favorites-pl".to_string(); let mut e = expanded.write(); if e.contains(&id) { e.remove(&id); } else { e.insert(id); } },
                                }
                                if expanded.read().contains("favorites-pl") {
                                    PlaylistCarousel {
                                        row_id: "row-favorites",
                                        items: favorites,
                                        on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(false); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryPlaylistItems(id, 0))); },
                                        on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadPlaylistInQueue(id))),
                                        on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddPlaylistToQueue(id))),
                                    }
                                }
                            }

                            // By Genre (lazy loaded)
                            {genre_headers.iter().enumerate().map(|(idx, (genre, count))| {
                                let genre = genre.clone();
                                let slug = pl_slug(&genre);
                                let section_id = format!("genre-pl-{slug}");
                                let is_exp = expanded.read().contains(&section_id);
                                let sid = section_id.clone();
                                let g1 = genre.clone(); let g2 = genre.clone(); let g3 = genre.clone();
                                let albums = lazy_genre_albums.read().get(&genre).cloned();
                                let item_count = albums.as_ref().map_or(*count, Vec::len);
                                rsx! {
                                    SectionHeaderWithActions {
                                        key: "gh-{idx}-{section_id}",
                                        title: genre, count: item_count, is_expanded: is_exp,
                                        on_toggle: move |()| { let mut e = expanded.write(); if e.contains(&sid) { e.remove(&sid); } else { e.insert(sid.clone()); if !lazy_genre_albums.read().contains_key(&g1) { ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumsByGenre(g1.clone()))); } } },
                                        on_load: move |()| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadGenreInQueue(g2.clone()))),
                                        on_add:  move |()| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddGenreToQueue(g3.clone()))),
                                    }
                                    if is_exp {
                                        if let Some(albums) = albums {
                                            RawAlbumCarousel {
                                                key: "gc-{idx}-{section_id}", row_id: "row-{section_id}", albums,
                                                on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(true); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumItems(id, 0))); },
                                                on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadAlbumInQueue(id))),
                                                on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddAlbumToQueue(id))),
                                            }
                                        } else {
                                            div { class: "flex justify-center py-4", span { class: "loading loading-spinner loading-sm" } }
                                        }
                                    }
                                }
                            })}

                            // By Decade (lazy loaded)
                            {decade_headers.iter().enumerate().map(|(idx, (decade, count))| {
                                let decade = decade.clone();
                                let slug = pl_slug(&decade);
                                let section_id = format!("decade-pl-{slug}");
                                let is_exp = expanded.read().contains(&section_id);
                                let sid = section_id.clone();
                                let d1 = decade.clone(); let d2 = decade.clone(); let d3 = decade.clone();
                                let albums = lazy_decade_albums.read().get(&decade).cloned();
                                let item_count = albums.as_ref().map_or(*count, Vec::len);
                                rsx! {
                                    SectionHeaderWithActions {
                                        key: "dh-{idx}-{section_id}",
                                        title: decade, count: item_count, is_expanded: is_exp,
                                        on_toggle: move |()| { let mut e = expanded.write(); if e.contains(&sid) { e.remove(&sid); } else { e.insert(sid.clone()); if !lazy_decade_albums.read().contains_key(&d1) { ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumsByDecade(d1.clone()))); } } },
                                        on_load: move |()| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadDecadeInQueue(d2.clone()))),
                                        on_add:  move |()| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddDecadeToQueue(d3.clone()))),
                                    }
                                    if is_exp {
                                        if let Some(albums) = albums {
                                            RawAlbumCarousel {
                                                key: "dc-{idx}-{section_id}", row_id: "row-{section_id}", albums,
                                                on_open: move |(id, name): (String, String)| { ui.playlist_modal_id.set(Some(id.clone())); ui.playlist_modal_name.set(name); ui.playlist_modal_is_album.set(true); ui.playlist_modal_open.set(true); ws_send(&ws, &UserCommand::Playlist(PlaylistCommand::QueryAlbumItems(id, 0))); },
                                                on_load: move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadAlbumInQueue(id))),
                                                on_add:  move |id: String| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddAlbumToQueue(id))),
                                            }
                                        } else {
                                            div { class: "flex justify-center py-4", span { class: "loading loading-spinner loading-sm" } }
                                        }
                                    }
                                }
                            })}
                        }
                    }
                }
            }
        }
    }
}

// ── Section header ────────────────────────────────────────────────────────────

#[component]
fn SectionHeader(title: String, count: usize, is_expanded: bool, on_toggle: EventHandler) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2 px-3 py-2 cursor-pointer select-none hover:bg-base-200 border-b border-base-300",
            onclick: move |_| on_toggle.call(()),
            i {
                class: "material-icons text-base-content/60 transition-transform duration-200",
                style: if is_expanded { "transform:rotate(0deg)" } else { "transform:rotate(-90deg)" },
                "expand_more"
            }
            span { class: "font-semibold text-sm flex-1", "{title}" }
            span { class: "text-xs text-base-content/40", "({count})" }
        }
    }
}

// ── Section header with queue actions ────────────────────────────────────────

#[component]
fn SectionHeaderWithActions(
    title: String,
    count: usize,
    is_expanded: bool,
    on_toggle: EventHandler,
    on_load: EventHandler,
    on_add: EventHandler,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1 sm:gap-2 px-2 sm:px-3 py-2 cursor-pointer select-none hover:bg-base-200 border-b border-base-300 overflow-hidden",
            onclick: move |_| on_toggle.call(()),
            i {
                class: "material-icons text-base-content/60 transition-transform duration-200 text-lg shrink-0",
                style: if is_expanded { "transform:rotate(0deg)" } else { "transform:rotate(-90deg)" },
                "expand_more"
            }
            span { class: "font-semibold text-sm min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap", "{title}" }
            span { class: "text-xs text-base-content/40 hidden sm:inline shrink-0", "({count})" }
            div { class: "flex gap-1 shrink-0",
                button { class: "btn btn-ghost btn-xs h-6 w-6 p-0 min-h-0", title: "Load all", onclick: move |e| { e.stop_propagation(); on_load.call(()); }, i { class: "material-icons text-xs", "playlist_play" } }
                button { class: "btn btn-ghost btn-xs h-6 w-6 p-0 min-h-0", title: "Add all",  onclick: move |e| { e.stop_propagation(); on_add.call(()); },  i { class: "material-icons text-xs", "playlist_add" } }
            }
        }
    }
}

// ── Horizontally scrollable row ───────────────────────────────────────────────

/// `(can_scroll_left, can_scroll_right)` of the row, `None` when it is not in
/// the DOM (yet).
fn scroll_state(id: &str) -> Option<(bool, bool)> {
    let el = web_sys::window()?.document()?.get_element_by_id(id)?;
    let left = el.scroll_left();
    Some((left > 0, left + el.client_width() < el.scroll_width() - 1))
}

/// Scroll the row by most of its visible width; `factor` is -1 or 1.
/// The smooth animation comes from daisyUI's `.carousel` CSS.
fn scroll_row_by(id: &str, factor: f64) {
    let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(id)) else {
        return;
    };
    el.scroll_by_with_x_and_y(f64::from(el.client_width()) * 0.8 * factor, 0.0);
}

/// Wraps a cover row in a scroll container with prev/next buttons. Touch
/// devices can swipe the row, but with a mouse there is no other way to reach
/// the covers beyond the fold: daisyUI's `.carousel` hides the scrollbar.
#[component]
fn ScrollRow(id: String, children: Element) -> Element {
    let mut can_left = use_signal(|| false);
    let mut can_right = use_signal(|| false);

    // A signal, not the `String` itself, so the closure below stays `Copy`.
    let row_id = use_signal(|| id.clone());
    let mut refresh = move || {
        if let Some((left, right)) = scroll_state(row_id.read().as_str()) {
            can_left.set(left);
            can_right.set(right);
        }
    };
    // Measure once the row (and any later items) are in the DOM.
    use_effect(move || refresh());

    let prev_id = id.clone();
    let next_id = id.clone();
    rsx! {
        // `onmouseenter` re-measures after a window resize, which no scroll
        // event would report.
        div { class: "relative", onmouseenter: move |_| refresh(),
            div {
                id: "{id}",
                class: "carousel carousel-center gap-3 w-full px-3 py-3",
                onscroll: move |_| refresh(),
                {children}
            }
            if can_left() {
                button {
                    class: "btn btn-circle btn-sm absolute left-1 top-1/2 -translate-y-1/2 bg-base-100/80 border-none shadow-md hidden sm:flex",
                    title: "Scroll left",
                    onclick: move |_| scroll_row_by(&prev_id, -1.0),
                    i { class: "material-icons text-base", "chevron_left" }
                }
            }
            if can_right() {
                button {
                    class: "btn btn-circle btn-sm absolute right-1 top-1/2 -translate-y-1/2 bg-base-100/80 border-none shadow-md hidden sm:flex",
                    title: "Scroll right",
                    onclick: move |_| scroll_row_by(&next_id, 1.0),
                    i { class: "material-icons text-base", "chevron_right" }
                }
            }
        }
    }
}

// ── Album carousel (PlaylistType wrapper) ─────────────────────────────────────

#[component]
fn AlbumCarousel(
    row_id: String,
    items: Vec<PlaylistType>,
    on_open: EventHandler<(String, String)>,
    on_load: EventHandler<String>,
    on_add: EventHandler<String>,
) -> Element {
    let albums: Vec<Album> = items
        .iter()
        .filter_map(|it| match it {
            PlaylistType::LatestRelease(a) | PlaylistType::RecentlyAdded(a) => Some(a.clone()),
            _ => None,
        })
        .collect();
    rsx! { RawAlbumCarousel { row_id, albums, on_open, on_load, on_add } }
}

// ── Raw album carousel ────────────────────────────────────────────────────────

#[component]
fn RawAlbumCarousel(
    row_id: String,
    albums: Vec<Album>,
    on_open: EventHandler<(String, String)>,
    on_load: EventHandler<String>,
    on_add: EventHandler<String>,
) -> Element {
    rsx! {
        ScrollRow { id: row_id,
            {albums.iter().map(|album| {
                let id = album.id.clone(); let id2 = id.clone(); let id3 = id.clone();
                let title = album.title.clone(); let title2 = title.clone();
                let artist = album.artist.clone().unwrap_or_default();
                let year = album.released.as_ref().map(|d| format!("{}", d.format("%Y"))).unwrap_or_default();
                let img_src = album.image_id.as_ref().map_or_else(|| "/no_album.svg".to_string(), |i| format!("/artwork/{i}"));
                rsx! {
                    div { key: "{id}", class: "carousel-item",
                        div { class: "card w-32 bg-base-200 cursor-pointer hover:bg-base-300 transition shadow-sm group",
                            onclick: move |_| on_open.call((id.clone(), title.clone())),
                            figure { class: "relative w-32 h-32 overflow-hidden rounded-t-lg",
                                img { class: "w-full h-full object-cover", src: "{img_src}", alt: "{title2}" }
                                div { class: "absolute inset-0 bg-black/50 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 transition flex items-center justify-center gap-1",
                                    button { class: "btn btn-circle btn-sm btn-primary", title: "Load", onclick: move |e| { e.stop_propagation(); on_load.call(id2.clone()); }, i { class: "material-icons text-sm", "playlist_play" } }
                                    button { class: "btn btn-circle btn-sm btn-ghost bg-black/40", title: "Add", onclick: move |e| { e.stop_propagation(); on_add.call(id3.clone()); }, i { class: "material-icons text-sm", "playlist_add" } }
                                }
                            }
                            div { class: "card-body p-2",
                                p { class: "text-xs font-semibold truncate leading-tight", "{title2}" }
                                if !artist.is_empty() { p { class: "text-xs text-base-content/50 truncate", "{artist}" } }
                                if !year.is_empty()   { p { class: "text-xs text-base-content/30", "{year}" } }
                            }
                        }
                    }
                }
            })}
        }
    }
}

// ── Playlist carousel ─────────────────────────────────────────────────────────

#[component]
fn PlaylistCarousel(
    row_id: String,
    items: Vec<PlaylistType>,
    on_open: EventHandler<(String, String)>,
    on_load: EventHandler<String>,
    on_add: EventHandler<String>,
) -> Element {
    rsx! {
        ScrollRow { id: row_id,
            {items.iter().filter_map(|it| {
                let (id, name, image_id) = match it {
                    PlaylistType::Saved(p) | PlaylistType::MostPlayed(p) | PlaylistType::Liked(p) | PlaylistType::Featured(p)
                        => (p.id.clone(), p.name.clone(), p.image.clone()),
                    _ => return None,
                };
                let id2 = id.clone(); let id3 = id.clone(); let name2 = name.clone();
                let img_src = image_id.map_or_else(|| "/no_album.svg".to_string(), |i| format!("/artwork/{i}"));
                Some(rsx! {
                    div { key: "{id}", class: "carousel-item",
                        div { class: "card w-32 bg-base-200 cursor-pointer hover:bg-base-300 transition shadow-sm group",
                            onclick: move |_| on_open.call((id.clone(), name.clone())),
                            figure { class: "relative w-32 h-32 overflow-hidden rounded-t-lg bg-base-300",
                                img { class: "w-full h-full object-cover", src: "{img_src}", alt: "{name2}" }
                                div { class: "absolute inset-0 bg-black/50 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 transition flex items-center justify-center gap-1",
                                    button { class: "btn btn-circle btn-sm btn-primary", title: "Load", onclick: move |e| { e.stop_propagation(); on_load.call(id2.clone()); }, i { class: "material-icons text-sm", "playlist_play" } }
                                    button { class: "btn btn-circle btn-sm btn-ghost bg-black/40", title: "Add", onclick: move |e| { e.stop_propagation(); on_add.call(id3.clone()); }, i { class: "material-icons text-sm", "playlist_add" } }
                                }
                            }
                            div { class: "card-body p-2",
                                p { class: "text-xs font-semibold truncate leading-tight", "{name2}" }
                                p { class: "text-xs text-base-content/50", "Playlist" }
                            }
                        }
                    }
                })
            })}
        }
    }
}
