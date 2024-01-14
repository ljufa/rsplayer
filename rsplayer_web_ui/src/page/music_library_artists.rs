use api_models::{
    common::{MetadataLibraryItem, UserCommand},
    state::StateChangeEvent,
};
use indextree::{Arena, NodeId};
use seed::{div, empty, i, li, p, prelude::*, section, span, style, ul, C, IF};

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
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        api_models::common::MetadataCommand::QueryArtists,
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
    view_files(model)
}

fn view_files(model: &Model) -> Node<Msg> {
    section![
        view_spinner_modal(model.wait_response),
        C!["section"],
        ul![C!["wtree"], get_tree_start_node(model.tree.root, &model.tree.arena,)],
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
            label = name.clone();
            is_dir = true;
        }
        MetadataLibraryItem::Album { name, year: _ } => {
            label = name.to_string();
            is_dir = true;
        }
        _ => {}
    };
    if !is_root {
        let left_position = if is_dir { "20px" } else { "0px" };
        span.add_child(div![
            C!["level", "is-mobile"],
            div![
                C!["level-left", "is-flex-grow-3"],
                style! {
                    St::Height => node_height,
                },
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
