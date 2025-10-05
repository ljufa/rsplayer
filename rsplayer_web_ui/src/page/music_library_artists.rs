use api_models::{
    common::{MetadataLibraryItem, UserCommand},
    state::StateChangeEvent,
};
use indextree::{Arena, NodeId};
use seed::{
    a, attrs, div, empty, i, input, li, p,
    prelude::{web_sys::KeyboardEvent, *},
    section, span, style, ul, C, IF,
};

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
}

#[derive(Debug)]
pub struct Model {
    tree: TreeModel,
    wait_response: bool,
    search_input: String,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    if let Some(search_term) = url.search().get("search").and_then(|v| v.get(0).cloned()) {
        if !search_term.is_empty() {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::SearchArtists(search_term.clone()),
            )));
            return Model {
                tree: TreeModel::new(),
                wait_response: true,
                search_input: search_term,
            };
        }
    }
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        api_models::common::MetadataCommand::QueryArtists,
    )));
    Model {
        tree: TreeModel::new(),
        wait_response: true,
        search_input: String::new(),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::MetadataLocalItems(result)) => {
            model.wait_response = false;
            result.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(item);
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        Msg::ExpandNodeClick(id) => {
            model.wait_response = true;
            model.tree.current = id;
            match model.tree.arena.get(id).unwrap().get() {
                MetadataLibraryItem::Artist { name } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                        api_models::common::MetadataCommand::QueryAlbumsByArtist(name.to_owned()),
                    )));
                }
                MetadataLibraryItem::Album { name, year: _ } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                        api_models::common::MetadataCommand::QuerySongsByAlbum(name.to_owned()),
                    )));
                }
                MetadataLibraryItem::SongItem(_) => {}
                _ => {}
            }
        }
        Msg::CollapseNodeClick(id) => {
            let arena = model.tree.arena.clone();
            let children = id.children(&arena);
            for c in children {
                c.remove_subtree(&mut model.tree.arena);
            }
        }
        Msg::AddItemToQueue(id) => {
            let item = model.tree.arena.get(id).unwrap().get();
            match item {
                MetadataLibraryItem::SongItem(song) => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::AddSongToQueue(song.file.clone()),
                    )));
                }
                MetadataLibraryItem::Album { name, year: _ } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::AddAlbumToQueue(name.to_owned()),
                    )));
                }
                MetadataLibraryItem::Artist { name } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::AddArtistToQueue(name.to_owned()),
                    )));
                }
                _ => {}
            }
        }
        Msg::LoadItemToQueue(id) => {
            let item = model.tree.arena.get(id).unwrap().get();
            match item {
                MetadataLibraryItem::SongItem(song) => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::LoadSongToQueue(song.file.clone()),
                    )));
                }
                MetadataLibraryItem::Album { name, year: _ } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::LoadAlbumInQueue(name.to_owned()),
                    )));
                }
                MetadataLibraryItem::Artist { name } => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::LoadArtistInQueue(name.to_owned()),
                    )));
                }
                _ => {}
            }
        }
        Msg::WebSocketOpen => {
            if model.search_input.is_empty() {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                    api_models::common::MetadataCommand::QueryArtists,
                )));
            }
        }
        Msg::SearchInputChanged(term) => {
            model.search_input = term;
            orders.skip();
        }
        Msg::DoSearch => {
            model.wait_response = true;
            model.tree = TreeModel::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::SearchArtists(model.search_input.clone()),
            )));
        }
        Msg::ClearSearch => {
            model.wait_response = true;
            model.tree = TreeModel::new();
            model.search_input = String::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::QueryArtists,
            )));
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
                    At::Placeholder => "Find artist",
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
        ul![C!["wtree"], get_tree_start_node(model.tree.root, &model.tree.arena)],
    ]
}

#[allow(clippy::collection_is_never_read)]
fn get_tree_start_node(node_id: NodeId, arena: &Arena<MetadataLibraryItem>) -> Node<Msg> {
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
    let is_root = false;
    match item {
        MetadataLibraryItem::SongItem(song) => {
            label = song.get_file_name_without_path();
        }
        MetadataLibraryItem::Artist { name } => {
            label.clone_from(name);
            is_dir = true;
        }
        MetadataLibraryItem::Album { name, year: _ } => {
            label = name.to_string();
            is_dir = true;
        }
        _ => {}
    };
    let show_expand_button = is_dir;
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
            ul.add_child(get_tree_start_node(c, arena));
        }
        li.add_child(ul);
    }
    li
}
