use api_models::{
    common::{
        MetadataLibraryItem,
        QueueCommand::{AddLocalLibDirectory, LoadLocalLibDirectory},
        UserCommand,
    },
    state::StateChangeEvent,
};

use indextree::{Arena, NodeId};
use seed::{div, empty, i, li, p, prelude::*, section, span, ul, C, IF};

use crate::view_spinner_modal;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    WebSocketOpen,
    SendUserCommand(UserCommand),
    StatusChangeEventReceived(StateChangeEvent),
    ExpandNodeClick(NodeId),
    CollapseNodeClick(NodeId),
    AddItemToQueue(NodeId),
    LoadItemToQueue(NodeId),
    Playlists(crate::page::music_library_static_playlist::Msg),
}

#[derive(Debug)]
pub struct TreeModel {
    arena: Arena<MetadataLibraryItem>,
    root: NodeId,
    current: NodeId,
}
impl TreeModel {
    fn new() -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(MetadataLibraryItem::Empty);
        TreeModel {
            arena,
            root,
            current: root,
        }
    }
    fn get_full_path(&self, id: NodeId) -> String {
        let path: String = id.ancestors(&self.arena).fold(String::new(), |mut acc, n| {
            let node = self.arena.get(n).unwrap().get();
            acc.insert_str(0, &node.get_id());
            acc
        });
        path
    }
}
#[derive(Debug)]
pub struct Model {
    tree: TreeModel,
    wait_response: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
    )));
    Model {
        tree: TreeModel::new(),
        wait_response: true,
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::MetadataLocalItems(result)) => {
            model.wait_response = false;
            result.items.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(item);
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        Msg::ExpandNodeClick(id) => {
            model.wait_response = true;
            model.tree.current = id;
            let path = model.tree.get_full_path(id);
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::QueryLocalFiles(path, 0),
            )));
        }
        Msg::CollapseNodeClick(id) => {
            let arena = model.tree.arena.clone();
            let children = id.children(&arena);
            for c in children {
                c.remove_subtree(&mut model.tree.arena);
            }
        }
        Msg::AddItemToQueue(id) => {
            let path = model.tree.get_full_path(id);
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(AddLocalLibDirectory(path))));
        }
        Msg::LoadItemToQueue(id) => {
            let path = model.tree.get_full_path(id);
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(LoadLocalLibDirectory(path))));
        }
        Msg::WebSocketOpen => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
            )));
        }
        _ => {
            orders.skip();
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![div![C!["columns"], div![C!["column"], view_content(model)],]]
}

#[allow(clippy::match_same_arms)]
fn view_content(model: &Model) -> Node<Msg> {
    div![view_files(model),]
}

fn view_files(model: &Model) -> Node<Msg> {
    section![
        view_spinner_modal(model.wait_response),
        C!["section"],
        ul![C!["wtree"], get_tree_start_node(model.tree.root, &model.tree.arena)],
    ]
}
fn get_tree_start_node(node_id: NodeId, arena: &Arena<MetadataLibraryItem>) -> Node<Msg> {
    let Some(value) = arena.get(node_id) else {
        return empty!();
    };
    let item = value.get();
    let children: Vec<NodeId> = node_id.children(arena).collect();
    let mut li: Node<Msg> = li![];
    let mut span: Node<Msg> = span![C!["has-background-dark-transparent"]];
    let mut label = String::new();
    let mut is_dir = false;
    let mut is_root = false;
    match item {
        MetadataLibraryItem::SongItem(song) => {
            label = song.get_file_name_without_path();
        }
        MetadataLibraryItem::Directory { name } => {
            label = name.clone();
            is_dir = true;
        }
        MetadataLibraryItem::Empty => {
            is_root = true;
        }
    };
    if !is_root {
        span.add_child(
            div![
            C!["level", "is-mobile"],
            div![
                C!["level-left"],
                IF!(is_dir =>
                    if children.is_empty() {
                        i![C!["material-icons"], "expand_more"]
                    } else {
                        i![C!["material-icons"], "expand_less"]
                    }
                ),
                IF!(is_dir =>
                    if children.is_empty() {
                            ev(Ev::Click, move |_| Msg::ExpandNodeClick(node_id))
                    } else {
                            ev(Ev::Click, move |_| Msg::CollapseNodeClick(node_id))
                    }
                ),
                div![
                    C!["level-item", "is-flex-grow-3"],
                    p![C!["has-overflow-ellipsis-text"], label]
                ],
            ],
            div![
                C!["level-right"],
                div![
                    C!["level-item", "mr-5"],
                    i![C!["material-icons"], "playlist_add"],
                    ev(Ev::Click, move |_| Msg::AddItemToQueue(node_id))
                ],
                div![
                    C!["level-item"],
                    i![C!["material-icons"], "play_circle_filled"],
                    ev(Ev::Click, move |_| Msg::LoadItemToQueue(node_id))
                ],
            ],
            ]
        );
    }

    li.add_child(span);
    if !children.is_empty() {
        let mut ul: Node<Msg> = ul!();
        for c in children {
            ul.add_child(get_tree_start_node(c, arena));
        }
        li.add_child(ul);
    }
    li
}

#[cfg(test)]
mod test {

    use indextree::Arena;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn traverse_tree() {
        let arena = &mut Arena::new();
        let l1 = arena.new_node("L1");
        let l21 = arena.new_node("L21");
        let l22 = arena.new_node("L22");
        let l31 = arena.new_node("L31");
        let l32 = arena.new_node("L32");
        l21.append(l31, arena);
        l1.append(l22, arena);
        l1.append(l21, arena);
        l21.append(l32, arena);
        let l321 = arena.new_node("L321");
        l31.append(l321, arena);
        l32.append(arena.new_node("L331"), arena);
        l321.append(arena.new_node("L3311"), arena);
        l321.append(arena.new_node("L3312"), arena);
        l22.append(arena.new_node("L221"), arena);
        l22.append(arena.new_node("L222"), arena);
        // let tree = get_tree_start_node(l1, arena);
        // log!(format!("tree: {}", tree));
    }
}
