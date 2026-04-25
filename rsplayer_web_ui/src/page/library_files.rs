use api_models::common::{MetadataCommand, MetadataLibraryItem, QueueCommand, UserCommand};
use dioxus::prelude::*;
use indextree::{Arena, NodeId};
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

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

    fn full_path(&self, id: NodeId) -> String {
        id.ancestors(&self.arena).fold(String::new(), |mut acc, n| {
            let item = self.arena.get(n).unwrap().get();
            acc.insert_str(0, &item.get_id());
            acc
        })
    }

    fn clear_children(&mut self, parent: NodeId) {
        let children: Vec<NodeId> = parent.children(&self.arena).collect();
        for c in children {
            c.remove_subtree(&mut self.arena);
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
pub fn LibraryFilesPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let mut tree: Signal<Tree> = use_signal(Tree::new);
    let mut loading = use_signal(|| true);
    let mut search = use_signal(String::new);

    // Check for ?search= query param
    let route_search = use_hook(|| {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|s| {
                s.split('&').chain(s.split('?')).find_map(|p| {
                    let p = p.trim_start_matches('?');
                    p.strip_prefix("search=").map(|v| v.to_string())
                })
            })
            .unwrap_or_default()
    });

    // Initial query
    use_effect(move || {
        if !route_search.is_empty() {
            search.set(route_search.clone());
            ws_send(
                &ws,
                &UserCommand::Metadata(MetadataCommand::SearchLocalFiles(route_search.clone(), 100)),
            );
        } else {
            ws_send(
                &ws,
                &UserCommand::Metadata(MetadataCommand::QueryLocalFiles(String::new(), 0)),
            );
        }
    });

    // React to incoming metadata items
    let metadata_items = state.metadata_local_items;
    use_effect(move || {
        let items = metadata_items.read().clone();
        if !items.is_empty() {
            let mut t = tree.write();
            let is_search = !search().is_empty();
            if is_search {
                let root = t.root;
                t.clear_children(root);
                t.append_items(root, items);
            } else {
                let current = t.current;
                t.clear_children(current);
                t.append_items(current, items);
            }
            *loading.write() = false;
        }
    });

    let mut do_search = move || {
        *loading.write() = true;
        let term = search();
        if term.is_empty() {
            *tree.write() = Tree::new();
            ws_send(
                &ws,
                &UserCommand::Metadata(MetadataCommand::QueryLocalFiles(String::new(), 0)),
            );
        } else {
            ws_send(
                &ws,
                &UserCommand::Metadata(MetadataCommand::SearchLocalFiles(term, 100)),
            );
        }
    };

    rsx! {
        div { class: "library-page",
            // ── Search bar ─────────────────────────────────────────────────
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300",
                input {
                    class: "input input-sm input-bordered flex-1",
                    r#type: "text",
                    placeholder: "Search files…",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                    onkeydown: move |e| { if e.key() == Key::Enter { do_search(); } },
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| do_search(),
                    i { class: "material-icons text-base", "search" }
                }
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| {
                        search.set(String::new());
                        *tree.write() = Tree::new();
                        *loading.write() = true;
                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryLocalFiles(String::new(), 0)));
                    },
                    i { class: "material-icons text-base", "backspace" }
                }
            }

            // ── Content ────────────────────────────────────────────────────
            if loading() {
                LibrarySkeleton {}
            } else {
                div { class: "overflow-y-auto",
                    {
                        let search_mode = !search().is_empty();
                        let search_str = search.read().clone();
                        let top_nodes: Vec<(NodeId, MetadataLibraryItem)> = {
                            let t = tree.read();
                            t.root.children(&t.arena)
                                .map(|id| (id, t.arena.get(id).unwrap().get().clone()))
                                .collect()
                        };
                        top_nodes.into_iter().map(move |(node_id, item)| {
                            rsx! {
                                LibraryNode {
                                    key: "{node_id:?}",
                                    item,
                                    node_id,
                                    search_mode,
                                    ws,
                                    tree,
                                    loading,
                                    search: search_str.clone(),
                                }
                            }
                        })
                    }
                }
            }
        }
    }
}

#[component]
fn LibraryNode(
    item: MetadataLibraryItem,
    node_id: NodeId,
    search_mode: bool,
    ws: Signal<Option<WebSocket>>,
    tree: Signal<Tree>,
    loading: Signal<bool>,
    search: String,
) -> Element {
    let is_dir = item.is_dir();
    let label = item.get_title();
    let has_children = node_id.children(&tree.read().arena).count() > 0;

    let mut toggle_dir = move |_| {
        let t = tree.read();
        let has_children = node_id.children(&t.arena).count() > 0;

        if has_children {
            drop(t);
            tree.write().clear_children(node_id);
        } else {
            let path = t.full_path(node_id);
            drop(t);
            tree.write().current = node_id;
            *loading.write() = true;
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryLocalFiles(path, 0)));
        }
    };

    rsx! {
        div { class: "library-node",
            div {
                class: "library-node__row flex items-center gap-1 pl-3 pr-2 py-1.5 hover:bg-base-200 group",
                onclick: move |_| {
                    if is_dir {
                        toggle_dir(());
                    }
                },
                if is_dir {
                    button {
                        class: "btn btn-ghost btn-xs px-1",
                        onclick: move |e| {
                            e.stop_propagation();
                            toggle_dir(());
                        },
                        i { class: "material-icons text-sm",
                            if node_id.children(&tree.read().arena).count() > 0 { "folder_open" } else { "folder" }
                        }
                    }
                } else {
                    span { class: "w-7 flex justify-center",
                        i { class: "material-icons text-sm text-base-content/40", "music_note" }
                    }
                }
                span { class: "flex-1 text-sm truncate", "{label}" }
                div { class: "ml-auto flex items-center gap-1",
                    if is_dir {
                        button {
                            class: "btn btn-ghost btn-xs",
                            title: "Load directory to queue",
                            onclick: move |e| {
                                e.stop_propagation();
                                ws_send(&ws, &UserCommand::Queue(QueueCommand::LoadLocalLibDirectory(tree.read().full_path(node_id))));
                            },
                            i { class: "material-icons text-sm", "playlist_play" }
                        }
                        button {
                            class: "btn btn-ghost btn-xs",
                            title: "Add directory to queue",
                            onclick: move |e| {
                                e.stop_propagation();
                                let path = tree.read().full_path(node_id);
                                ws_send(&ws, &UserCommand::Queue(QueueCommand::AddLocalLibDirectory(path)));
                            },
                            i { class: "material-icons text-sm", "playlist_add" }
                        }
                    } else {
                        button {
                            class: "btn btn-ghost btn-xs",
                            title: "Add to queue",
                            onclick: {
                                let item = item.clone();
                                move |e| {
                                    e.stop_propagation();
                                    if let MetadataLibraryItem::SongItem(ref song) = item {
                                        ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongToQueue(song.file.clone())));
                                    }
                                }
                            },
                            i { class: "material-icons text-sm", "playlist_add" }
                        }
                        button {
                            class: "btn btn-ghost btn-xs",
                            title: "Play next",
                            onclick: {
                                let item = item.clone();
                                move |e| {
                                    e.stop_propagation();
                                    if let MetadataLibraryItem::SongItem(ref song) = item {
                                        ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongAfterCurrent(song.file.clone())));
                                    }
                                }
                            },
                            i { class: "material-icons text-sm", "playlist_play" }
                        }
                        button {
                            class: "btn btn-ghost btn-xs",
                            title: "Add and play",
                            onclick: {
                                let item = item.clone();
                                move |e| {
                                    e.stop_propagation();
                                    if let MetadataLibraryItem::SongItem(ref song) = item {
                                        ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongAndPlay(song.file.clone())));
                                    }
                                }
                            },
                            i { class: "material-icons text-sm", "play_arrow" }
                        }
                    }
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
                        children.into_iter().map(move |(child_id, child_item)| {
                            rsx! {
                                LibraryNode {
                                    key: "{child_id:?}",
                                    item: child_item,
                                    node_id: child_id,
                                    search_mode,
                                    ws,
                                    tree,
                                    loading,
                                    search: search.clone(),
                                }
                            }
                        })
                    }
                }
            }
        }
    }
}

#[component]
fn LibrarySkeleton() -> Element {
    rsx! {
        div { class: "flex flex-col gap-1 p-3",
            {(0..10).map(|_| rsx! {
                div { class: "flex items-center gap-2 py-1.5 px-2",
                    div { class: "skeleton w-5 h-5 rounded" }
                    div { class: "skeleton h-4 flex-1 rounded" }
                }
            })}
        }
    }
}
