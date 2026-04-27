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
pub fn LibraryArtistsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let mut tree: Signal<Tree> = use_signal(Tree::new);
    let mut loading = use_signal(|| true);
    let mut search = use_signal(String::new);

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

    use_effect(move || {
        if !route_search.is_empty() {
            search.set(route_search.clone());
            ws_send(
                &ws,
                &UserCommand::Metadata(MetadataCommand::SearchArtists(route_search.clone())),
            );
        } else {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryArtists));
        }
    });

    let metadata_items = state.metadata_local_items;
    use_effect(move || {
        let items = metadata_items.read().clone();
        if !items.is_empty() {
            let mut t = tree.write();
            let current = t.current;
            t.clear_children(current);
            t.append_items(current, items);
            drop(t);
            *loading.write() = false;
        }
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
                div { class: "overflow-y-auto",
                    {
                        let top_nodes: Vec<(NodeId, MetadataLibraryItem)> = {
                            let t = tree.read();
                            t.root.children(&t.arena)
                                .map(|id| (id, t.arena.get(id).unwrap().get().clone()))
                                .collect()
                        };
                        top_nodes.into_iter().map(|(node_id, item)| {
                            rsx! {
                                ArtistNode {
                                    key: "{node_id:?}",
                                    item,
                                    node_id,
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

#[component]
fn ArtistNode(
    item: MetadataLibraryItem,
    node_id: NodeId,
    ws: Signal<Option<WebSocket>>,
    tree: Signal<Tree>,
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
        div { class: "library-node",
            div {
                class: "library-node__row flex items-center gap-1 pl-3 pr-2 py-1.5 hover:bg-base-200 group",
                onclick: {
                    let item = item.clone();
                    move |_| {
                        if !is_song {
                            let t = tree.read();
                            let has_children = node_id.children(&t.arena).count() > 0;
                            drop(t);
                            if has_children {
                                tree.write().clear_children(node_id);
                            } else {
                                tree.write().current = node_id;
                                match &item {
                                    MetadataLibraryItem::Artist { name } => {
                                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(name.clone())));
                                    }
                                    MetadataLibraryItem::Album { name, .. } => {
                                        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(name.clone())));
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
                                    tree.write().current = node_id;
                                    match &item {
                                        MetadataLibraryItem::Artist { name } => {
                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryAlbumsByArtist(name.clone())));
                                        }
                                        MetadataLibraryItem::Album { name, .. } => {
                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QuerySongsByAlbum(name.clone())));
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

    if is_song {
        let i2 = item.clone();
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
        let i2 = item.clone();
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
        (MetadataLibraryItem::Album { name, .. }, "add") => QueueCommand::AddAlbumToQueue(name.clone()),
        (MetadataLibraryItem::Album { name, .. }, "after") => QueueCommand::AddAlbumAfterCurrent(name.clone()),
        (MetadataLibraryItem::Album { name, .. }, "load") => QueueCommand::LoadAlbumInQueue(name.clone()),
        (MetadataLibraryItem::Album { name, .. }, "play") => QueueCommand::AddAlbumAndPlay(name.clone()),
        (MetadataLibraryItem::Artist { name }, "add") => QueueCommand::AddArtistToQueue(name.clone()),
        (MetadataLibraryItem::Artist { name }, "after") => QueueCommand::AddArtistAfterCurrent(name.clone()),
        (MetadataLibraryItem::Artist { name }, "load") => QueueCommand::LoadArtistInQueue(name.clone()),
        (MetadataLibraryItem::Artist { name }, "play") => QueueCommand::AddArtistAndPlay(name.clone()),
        _ => return,
    };
    ws_send(ws, &UserCommand::Queue(cmd));
}
