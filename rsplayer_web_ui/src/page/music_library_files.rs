use api_models::{
    common::{
        MetadataLibraryItem,
        QueueCommand::{AddLocalLibDirectory, LoadLocalLibDirectory},
        UserCommand,
    },
    state::StateChangeEvent,
};
use indextree::{Arena, NodeId};
use seed::{
    a, attrs, div, empty, i, input, li, p,
    prelude::{web_sys::KeyboardEvent, *},
    section, span, style, ul, C, IF,
};

use crate::{view_spinner_modal, Urls};

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
    SearchInputChanged(String),
    DoSearch,
    ClearSearch,
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
    fn get_full_path(&self, id: NodeId, is_search_mode: bool) -> String {
        let path: String = id.ancestors(&self.arena).fold(String::new(), |mut acc, n| {
            let node = self.arena.get(n).unwrap().get();
            if is_search_mode {
                if let MetadataLibraryItem::SongItem(song) = node {
                    acc.insert_str(0, &song.file);
                } else {
                    acc.insert_str(0, &node.get_id());
                }
            } else {
                acc.insert_str(0, &node.get_id());
            }
            acc
        });
        path
    }
}

#[derive(Debug)]
pub struct Model {
    tree: TreeModel,
    wait_response: bool,
    search_input: String,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    let search_term = Urls::get_search_term(&url);
    if let Some(term) = search_term {
        orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
            api_models::common::MetadataCommand::SearchLocalFiles(term.clone(), 100),
        )));
        Model {
            tree: TreeModel::new(),
            wait_response: true,
            search_input: term,
        }
    } else {
        orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
            api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
        )));
        Model {
            tree: TreeModel::new(),
            wait_response: true,
            search_input: String::new(),
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::MetadataLocalItems(result)) => {
            model.wait_response = false;
            if model.search_input.is_empty() {
                result.into_iter().for_each(|item| {
                    let node = model.tree.arena.new_node(item);
                    model.tree.current.append(node, &mut model.tree.arena);
                });
            } else {
                result.into_iter().for_each(|item| {
                    let node = model.tree.arena.new_node(item);
                    model.tree.root.append(node, &mut model.tree.arena);
                });
            }
        }
        Msg::ExpandNodeClick(id) => {
            model.wait_response = true;
            model.tree.current = id;
            let path = model.tree.get_full_path(id, false);
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
            let path = model.tree.get_full_path(id, !model.search_input.is_empty());
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(AddLocalLibDirectory(path))));
        }
        Msg::LoadItemToQueue(id) => {
            let path = model.tree.get_full_path(id, !model.search_input.is_empty());
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(LoadLocalLibDirectory(path))));
        }
        Msg::SearchInputChanged(term) => {
            orders.skip();
            model.search_input = term;
        }
        Msg::DoSearch => {
            model.wait_response = true;
            model.tree = TreeModel::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::SearchLocalFiles(model.search_input.clone(), 100),
            )));
        }
        Msg::ClearSearch => {
            model.wait_response = true;
            model.tree = TreeModel::new();
            model.search_input = String::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
            )));
        }

        Msg::WebSocketOpen => {
            if model.search_input.is_empty() {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                    api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
                )));
            }
        }
        _ => {
            orders.skip();
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![view_search_input(model), view_files(model)]
}

fn view_search_input(model: &Model) -> Node<Msg> {
    div![
        C!["transparent is-flex is-justify-content-center has-background-dark-transparent mt-2"],
        div![
            C!["control"],
            input![
                C!["input", "input-size"],
                attrs! {
                    At::Value => model.search_input,
                    At::Name => "search",
                    At::Type => "text",
                    At::Placeholder => "Find a song or directory",
                },
                input_ev(Ev::Input, Msg::SearchInputChanged),
                ev(Ev::KeyDown, |keyboard_event| {
                    if keyboard_event.value_of().to_string() == "[object KeyboardEvent]" {
                        let kev: KeyboardEvent = keyboard_event.unchecked_into();
                        IF!(kev.key_code() == 13 => Msg::DoSearch)
                    } else {
                        None
                    }
                }),
            ],
        ],
        div![
            C!["control"],
            a![
                attrs!(At::Title =>"Search"),
                i![C!["material-icons", "is-large-icon", "white-icon"], "search"],
                ev(Ev::Click, move |_| Msg::DoSearch)
            ],
            a![
                attrs!(At::Title =>"Clear search / Show all songs"),
                i![C!["material-icons", "is-large-icon", "white-icon"], "backspace"],
                ev(Ev::Click, move |_| Msg::ClearSearch)
            ],
        ],
    ]
}
fn view_files(model: &Model) -> Node<Msg> {
    section![
        view_spinner_modal(model.wait_response),
        C!["pr-2", "pl-1"],
        ul![
            C!["wtree"],
            get_tree_start_node(model.tree.root, &model.tree.arena, !model.search_input.is_empty())
        ],
    ]
}

#[allow(clippy::collection_is_never_read)]
fn get_tree_start_node(node_id: NodeId, arena: &Arena<MetadataLibraryItem>, is_search_mode: bool) -> Node<Msg> {
    let Some(value) = arena.get(node_id) else {
        return empty!();
    };
    let item = value.get();
    let children: Vec<NodeId> = node_id.children(arena).collect();
    let mut li: Node<Msg> = li![];
    let node_height = "40px";
    let mut span: Node<Msg> = span![
        C!["has-background-dark-transparent"],
        style! {
            St::Height => node_height,
        },
    ];
    let mut label = String::new();
    let mut is_dir = false;
    let mut is_root = false;
    match item {
        MetadataLibraryItem::SongItem(song) => {
            label = song.get_file_name_without_path();
        }
        MetadataLibraryItem::Directory { name } => {
            label.clone_from(name);
            is_dir = true;
        }
        MetadataLibraryItem::Empty => {
            is_root = true;
        }
        _ => {}
    };
    let show_expand_button = is_dir && !is_search_mode;
    if !is_root {
        let left_position = if show_expand_button { "20px" } else { "0px" };
        span.add_child(div![
            C!["level", "is-mobile"],
            div![
                C!["level-left", "is-flex-grow-3"],
                style! {
                    St::Height => node_height,
                },
                IF!(show_expand_button =>

                    if children.is_empty() {
                        i![C!["material-icons"], "expand_more"]
                    } else {
                        i![C!["material-icons"], "expand_less"]
                    }
                ),
                IF!(show_expand_button =>
                    if children.is_empty() {
                            ev(Ev::Click, move |_| Msg::ExpandNodeClick(node_id))
                    } else {
                            ev(Ev::Click, move |_| Msg::CollapseNodeClick(node_id))
                    }
                ),
                p![
                    C!["level-item"],
                    style! {
                        St::Position => "absolute",
                        St::Left => left_position,
                        St::Padding => "5px",
                        St::TextOverflow => "ellipsis",
                        St::Overflow => "hidden",
                        St::WhiteSpace => "nowrap",
                    },
                    label
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
                    C!["level-item", "mr-5"],
                    i![C!["material-icons"], "play_circle_filled"],
                    ev(Ev::Click, move |_| Msg::LoadItemToQueue(node_id))
                ],
            ],
        ]);
    }

    li.add_child(span);
    if !children.is_empty() {
        let mut ul: Node<Msg> = ul!();
        for c in children {
            ul.add_child(get_tree_start_node(c, arena, is_search_mode));
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
