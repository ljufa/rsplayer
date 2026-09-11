use api_models::common::{MetadataCommand, MetadataLibraryItem, QueueCommand, UserCommand};
use dioxus::prelude::*;
use indextree::{Arena, NodeId};
use unicode_normalization::UnicodeNormalization;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

/// Letters shown on the A–Z jump rail, in list order. `#` collects every
/// artist whose name does not start with an ASCII letter.
const JUMP_LETTERS: [char; 27] = [
    '#', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
    'W', 'X', 'Y', 'Z',
];

/// Minimum number of top-level artists before the jump rail is shown.
const JUMP_RAIL_MIN_ITEMS: usize = 20;

/// Bucket an artist name onto the jump rail. Mirrors the server's sort key
/// (NFD, combining marks stripped, case-folded, leading whitespace ignored)
/// so that jumping to a letter lands on the first artist sorted under it.
fn jump_bucket(name: &str) -> char {
    name.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .find(|c| !c.is_whitespace())
        .map_or('#', |c| if c.is_ascii_alphabetic() { c.to_ascii_uppercase() } else { '#' })
}

fn jump_index(letter: char) -> usize {
    JUMP_LETTERS.iter().position(|&l| l == letter).unwrap_or(0)
}

fn jump_anchor_id(letter: char) -> String {
    if letter == '#' {
        "artist-jump-hash".to_string()
    } else {
        format!("artist-jump-{letter}")
    }
}

/// Map a pointer's client Y coordinate over the rail to the nearest letter
/// that actually has artists. `None` when the rail is not in the DOM.
fn jump_letter_at(client_y: f64, present: &[bool; JUMP_LETTERS.len()]) -> Option<char> {
    let rail = web_sys::window()?.document()?.get_element_by_id("artist-jump-rail")?;
    let rect = rail.get_bounding_client_rect();
    if rect.height() <= 0.0 {
        return None;
    }
    let n = JUMP_LETTERS.len();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    let idx = (((client_y - rect.top()) / rect.height()) * n as f64).floor().clamp(0.0, (n - 1) as f64) as usize;
    (0..n).find_map(|d| {
        if idx + d < n && present[idx + d] {
            Some(JUMP_LETTERS[idx + d])
        } else if idx >= d && present[idx - d] {
            Some(JUMP_LETTERS[idx - d])
        } else {
            None
        }
    })
}

/// Scroll the window so the first artist under `letter` sits just below the
/// sticky nav bar. The page (not the list) is the scroll container.
fn jump_to(letter: char) {
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Some(el) = doc.get_element_by_id(&jump_anchor_id(letter)) else { return };
    let nav_height = doc
        .query_selector(".app-nav")
        .ok()
        .flatten()
        .map_or(0.0, |nav| nav.get_bounding_client_rect().height());
    let y = el.get_bounding_client_rect().top() + win.scroll_y().unwrap_or(0.0) - nav_height - 4.0;
    win.scroll_to_with_x_and_y(0.0, y.max(0.0));
}

struct Tree {
    arena: Arena<MetadataLibraryItem>,
    root: NodeId,
    current: NodeId,
}

impl Tree {
    fn new() -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(MetadataLibraryItem::Empty);
        Tree {
            arena,
            root,
            current: root,
        }
    }

    fn clear_children(&mut self, parent: NodeId) {
        let children: Vec<NodeId> = parent.children(&self.arena).collect();
        for c in children {
            c.remove_subtree(&mut self.arena);
        }
    }

    fn collapse_siblings(&mut self, node: NodeId) {
        let Some(parent) = self.arena.get(node).and_then(indextree::Node::parent) else {
            return;
        };
        let siblings: Vec<NodeId> = parent.children(&self.arena).filter(|&id| id != node).collect();
        for s in siblings {
            self.clear_children(s);
        }
    }

    fn append_items(&mut self, parent: NodeId, items: Vec<MetadataLibraryItem>) {
        for item in items {
            let node = self.arena.new_node(item);
            parent.append(node, &mut self.arena);
        }
    }
}

#[component]
pub fn LibraryArtistsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let mut tree: Signal<Tree> = use_signal(Tree::new);
    let mut loading = use_signal(|| true);
    let mut search = use_signal(String::new);
    // Letter currently under the pointer while scrubbing the A–Z rail.
    let jump_active: Signal<Option<char>> = use_signal(|| None);

    let route_search = use_hook(|| {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|s| {
                s.split('&').chain(s.split('?')).find_map(|p| {
                    let p = p.trim_start_matches('?');
                    p.strip_prefix("search=").map(std::string::ToString::to_string)
                })
            })
            .map(|raw| js_sys::decode_uri_component(&raw).map(String::from).unwrap_or(raw))
            .unwrap_or_default()
    });

    use_effect(move || {
        if route_search.is_empty() {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryArtists));
        } else {
            search.set(route_search.clone());
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::SearchArtists(route_search.clone())));
        }
    });

    let metadata_items = state.metadata_local_items;
    use_effect(move || {
        let items = metadata_items.read().clone();
        let mut t = tree.write();
        let current = t.current;
        t.clear_children(current);
        if !items.is_empty() {
            t.append_items(current, items);
        }
        drop(t);
        *loading.write() = false;
    });

    let mut do_search = move || {
        *loading.write() = true;
        let term = search();
        *tree.write() = Tree::new();
        if term.is_empty() {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryArtists));
        } else {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::SearchArtists(term)));
        }
    };

    rsx! {
        div { class: "library-page",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300",
                input {
                    class: "input input-sm input-bordered flex-1",
                    r#type: "text",
                    placeholder: "Search artists…",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                    onkeydown: move |e| { if e.key() == Key::Enter { do_search(); } },
                }
                button { class: "btn btn-sm btn-ghost", onclick: move |_| do_search(),
                    i { class: "material-icons text-base", "search" }
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| {
                        search.set(String::new());
                        *tree.write() = Tree::new();
                        *loading.write() = true;
                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryArtists));
                    },
                    i { class: "material-icons text-base", "backspace" }
                }
            }

            if loading() {
                div { class: "flex flex-col gap-1 p-3",
                    {(0..10).map(|_| rsx! {
                        div { class: "flex items-center gap-2 py-1.5 px-2",
                            div { class: "skeleton w-5 h-5 rounded" }
                            div { class: "skeleton h-4 flex-1 rounded" }
                        }
                    })}
                }
            } else {
                {
                    let (top_nodes, present) = {
                        let t = tree.read();
                        let mut seen = [false; JUMP_LETTERS.len()];
                        let nodes: Vec<(NodeId, MetadataLibraryItem, Option<String>)> = t
                            .root
                            .children(&t.arena)
                            .map(|id| {
                                let item = t.arena.get(id).unwrap().get().clone();
                                let bucket = jump_bucket(&item.get_title());
                                let slot = &mut seen[jump_index(bucket)];
                                let anchor = if *slot { None } else { *slot = true; Some(jump_anchor_id(bucket)) };
                                (id, item, anchor)
                            })
                            .collect();
                        (nodes, seen)
                    };
                    let show_rail = top_nodes.len() >= JUMP_RAIL_MIN_ITEMS;
                    if !search().is_empty() && top_nodes.is_empty() {
                        rsx! {
                            div { class: "flex flex-col items-center justify-center gap-2 p-8 text-base-content/60",
                                i { class: "material-icons text-4xl", "search_off" }
                                span { "No artists match your search." }
                                span { class: "text-sm", "Try a different search term." }
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "flex items-start",
                                // Right padding keeps rows clear of the fixed-position rail.
                                div { class: if show_rail { "flex-1 min-w-0 overflow-y-auto pr-7" } else { "flex-1 min-w-0 overflow-y-auto" },
                                    {
                                        top_nodes.into_iter().map(|(node_id, item, anchor)| {
                                            rsx! {
                                                ArtistNode {
                                                    key: "{node_id:?}",
                                                    item,
                                                    node_id,
                                                    ws,
                                                    tree,
                                                    anchor,
                                                }
                                            }
                                        })
                                    }
                                }
                                if show_rail {
                                    JumpRail { present, active: jump_active }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Vertical A–Z index next to the artist list. Tap a letter, or press and
/// drag along the rail, to scroll the list to the first artist under it.
#[component]
fn JumpRail(present: [bool; JUMP_LETTERS.len()], active: Signal<Option<char>>) -> Element {
    let mut scrub = move |client_y: f64| {
        if let Some(letter) = jump_letter_at(client_y, &present) {
            if active() != Some(letter) {
                active.set(Some(letter));
                jump_to(letter);
            }
        }
    };
    let mut release = move || active.set(None);

    rsx! {
        div {
            id: "artist-jump-rail",
            class: "artist-jump",
            role: "navigation",
            aria_label: "Jump to letter",
            onpointerdown: move |e: PointerEvent| {
                e.prevent_default();
                // Keep receiving pointer events while dragging outside the rail.
                if let Some(rail) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("artist-jump-rail"))
                {
                    let _ = rail.set_pointer_capture(e.data().pointer_id());
                }
                scrub(e.data().client_coordinates().y);
            },
            onpointermove: move |e: PointerEvent| {
                if active().is_some() {
                    scrub(e.data().client_coordinates().y);
                }
            },
            onpointerup: move |_| release(),
            onpointercancel: move |_| release(),
            for (i , letter) in JUMP_LETTERS.iter().enumerate() {
                span {
                    key: "{letter}",
                    class: if active() == Some(*letter) {
                        "artist-jump__letter is-active"
                    } else if present[i] {
                        "artist-jump__letter"
                    } else {
                        "artist-jump__letter is-empty"
                    },
                    "{letter}"
                }
            }
        }
        if let Some(letter) = active() {
            div { class: "artist-jump__bubble", aria_hidden: "true", "{letter}" }
        }
    }
}

#[component]
fn ArtistNode(
    item: MetadataLibraryItem,
    node_id: NodeId,
    ws: Signal<Option<WebSocket>>,
    tree: Signal<Tree>,
    /// DOM id set on the first artist of each jump-rail letter (top level only).
    #[props(default)]
    anchor: Option<String>,
) -> Element {
    let label = item.get_title();
    let is_song = matches!(item, MetadataLibraryItem::SongItem(_));
    let has_children = node_id.children(&tree.read().arena).count() > 0;
    let icon = match &item {
        MetadataLibraryItem::Artist { .. } => "person",
        MetadataLibraryItem::Album { .. } => "album",
        MetadataLibraryItem::SongItem(_) => "music_note",
        _ => "folder",
    };

    rsx! {
        div { class: "library-node", id: anchor,
            div {
                class: "library-node__row flex items-center gap-1 pl-3 pr-2 py-1.5 hover:bg-base-200 group",
                onclick: {
                    let item = item;
                    move |_| {
                        if !is_song {
                            let t = tree.read();
                            let has_children = node_id.children(&t.arena).count() > 0;
                            drop(t);
                            if has_children {
                                tree.write().clear_children(node_id);
                            } else {
                                {
                                    let mut tw = tree.write();
                                    tw.collapse_siblings(node_id);
                                    tw.current = node_id;
                                }
                                match &item {
                                    MetadataLibraryItem::Artist { name } => {
                                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(name.clone())));
                                    }
                                    MetadataLibraryItem::Album { .. } => {
                                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(item.get_id())));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                },
                if !is_song {
                    button {
                        class: "btn btn-ghost btn-xs px-1",
                        onclick: {
                            let item = item.clone();
                            move |e| {
                                e.stop_propagation();
                                let t = tree.read();
                                let has_children = node_id.children(&t.arena).count() > 0;
                                drop(t);
                                if has_children {
                                    tree.write().clear_children(node_id);
                                } else {
                                    {
                                        let mut tw = tree.write();
                                        tw.collapse_siblings(node_id);
                                        tw.current = node_id;
                                    }
                                    match &item {
                                        MetadataLibraryItem::Artist { name } => {
                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(name.clone())));
                                        }
                                        MetadataLibraryItem::Album { .. } => {
                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(item.get_id())));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        },
                        i { class: "material-icons text-sm",
                            if has_children { "expand_less" } else { "{icon}" }
                        }
                    }
                } else {
                    span { class: "w-7 flex justify-center",
                        i { class: "material-icons text-sm text-base-content/40", "music_note" }
                    }
                }
                span { class: "flex-1 text-sm truncate", "{label}" }
                div { class: "ml-auto flex items-center gap-1",
                    {queue_actions(item.clone(), ws)}
                }
            }
            if has_children {
                div { class: "library-node__children pl-4 border-l border-base-300 ml-4",
                    {
                        let children: Vec<(NodeId, MetadataLibraryItem)> = {
                            let t = tree.read();
                            node_id.children(&t.arena)
                                .map(|id| (id, t.arena.get(id).unwrap().get().clone()))
                                .collect()
                        };
                        children.into_iter().map(|(child_id, child_item)| {
                            rsx! {
                                ArtistNode {
                                    key: "{child_id:?}",
                                    item: child_item,
                                    node_id: child_id,
                                    ws,
                                    tree,
                                }
                            }
                        })
                    }
                }
            }
        }
    }
}

fn queue_actions(item: MetadataLibraryItem, ws: Signal<Option<WebSocket>>) -> Element {
    let is_song = matches!(item, MetadataLibraryItem::SongItem(_));
    let i2 = item.clone();

    if is_song {
        rsx! {
            button {
                class: "btn btn-ghost btn-xs",
                title: "Add to queue",
                onclick: move |_| send_queue_cmd(&item, &ws, "add"),
                i { class: "material-icons text-sm", "playlist_add" }
            }
            button {
                class: "btn btn-ghost btn-xs",
                title: "Play next",
                onclick: move |_| send_queue_cmd(&i2, &ws, "after"),
                i { class: "material-icons text-sm", "playlist_play" }
            }
        }
    } else {
        rsx! {
            button {
                class: "btn btn-ghost btn-xs",
                title: "Load to queue",
                onclick: move |_| send_queue_cmd(&item, &ws, "load"),
                i { class: "material-icons text-sm", "playlist_play" }
            }
            button {
                class: "btn btn-ghost btn-xs",
                title: "Add to queue",
                onclick: move |_| send_queue_cmd(&i2, &ws, "add"),
                i { class: "material-icons text-sm", "playlist_add" }
            }
        }
    }
}

fn send_queue_cmd(item: &MetadataLibraryItem, ws: &Signal<Option<WebSocket>>, action: &str) {
    let cmd = match (item, action) {
        (MetadataLibraryItem::SongItem(s), "add") => QueueCommand::AddSongToQueue(s.file.clone()),
        (MetadataLibraryItem::SongItem(s), "after") => QueueCommand::AddSongAfterCurrent(s.file.clone()),
        (MetadataLibraryItem::SongItem(s), "load") => QueueCommand::LoadSongToQueue(s.file.clone()),
        (MetadataLibraryItem::SongItem(s), "play") => QueueCommand::AddSongAndPlay(s.file.clone()),
        (MetadataLibraryItem::Album { .. }, "add") => QueueCommand::AddAlbumToQueue(item.get_id()),
        (MetadataLibraryItem::Album { .. }, "after") => QueueCommand::AddAlbumAfterCurrent(item.get_id()),
        (MetadataLibraryItem::Album { .. }, "load") => QueueCommand::LoadAlbumInQueue(item.get_id()),
        (MetadataLibraryItem::Album { .. }, "play") => QueueCommand::AddAlbumAndPlay(item.get_id()),
        (MetadataLibraryItem::Artist { name }, "add") => QueueCommand::AddArtistToQueue(name.clone()),
        (MetadataLibraryItem::Artist { name }, "after") => QueueCommand::AddArtistAfterCurrent(name.clone()),
        (MetadataLibraryItem::Artist { name }, "load") => QueueCommand::LoadArtistInQueue(name.clone()),
        (MetadataLibraryItem::Artist { name }, "play") => QueueCommand::AddArtistAndPlay(name.clone()),
        _ => return,
    };
    ws_send(ws, &UserCommand::Queue(cmd));
}

#[cfg(test)]
mod tests {
    use super::{jump_anchor_id, jump_bucket, jump_index, JUMP_LETTERS};

    #[test]
    fn buckets_follow_server_sort_key() {
        assert_eq!(jump_bucket("Yello"), 'Y');
        assert_eq!(jump_bucket("zz top"), 'Z');
        assert_eq!(jump_bucket("  Air"), 'A');
        assert_eq!(jump_bucket("Émilie Simon"), 'E');
        assert_eq!(jump_bucket("Ólafur Arnalds"), 'O');
    }

    #[test]
    fn non_letters_go_to_hash() {
        assert_eq!(jump_bucket("10cc"), '#');
        assert_eq!(jump_bucket("!!!"), '#');
        assert_eq!(jump_bucket(""), '#');
        assert_eq!(jump_bucket("Ørsted"), '#');
    }

    #[test]
    fn anchor_ids_are_unique_per_letter() {
        let ids: std::collections::HashSet<String> = JUMP_LETTERS.iter().map(|&l| jump_anchor_id(l)).collect();
        assert_eq!(ids.len(), JUMP_LETTERS.len());
        assert_eq!(jump_index('#'), 0);
        assert_eq!(jump_index('Z'), JUMP_LETTERS.len() - 1);
    }
}
