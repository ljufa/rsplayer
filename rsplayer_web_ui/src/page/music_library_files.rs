use api_models::{
    common::{
        MetadataLibraryItem,
        QueueCommand::{AddLocalLibDirectory, AddSongToQueue, LoadLocalLibDirectory, LoadSongToQueue},
        UserCommand,
    },
    state::StateChangeEvent,
};
use gloo_console::log;

use seed::{a, attrs, div, empty, i, li, nav, p, prelude::*, progress, section, span, style, ul, C, IF};

use crate::Urls;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    WebSocketOpen,
    SendUserCommand(UserCommand),
    StatusChangeEventReceived(StateChangeEvent),
    ListItemClick(String),
    BreadcrumbClick(usize),
    AddItemToQueue(String, bool),
    LoadItemToQueue(String, bool),

    Playlists(crate::page::music_library_static_playlist::Msg),
}
#[derive(Debug)]
pub struct FilesModel {
    items: Vec<MetadataLibraryItem>,
    current_dir_path: Vec<String>,
}
#[derive(Debug)]
pub struct Model {
    files_model: FilesModel,
    wait_response: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        api_models::common::MetadataCommand::QueryLocalFiles(String::new(), 0),
    )));

    Model {
        files_model: FilesModel {
            items: Vec::new(),
            current_dir_path: Vec::new(),
        },
        wait_response: true,
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::MetadataLocalItems(result)) => {
            model.files_model.items = result.items;
            model.wait_response = false;
        }
        Msg::ListItemClick(id) => {
            if !id.is_empty() {
                model.files_model.current_dir_path.push(id);
            }
            let mut current_dir = String::new();
            model.files_model.current_dir_path.iter().for_each(|p| {
                current_dir.push_str(p);
                current_dir.push('/');
            });
            model.wait_response = true;
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                api_models::common::MetadataCommand::QueryLocalFiles(current_dir, 0),
            )));
        }
        Msg::BreadcrumbClick(level) => {
            log!("Level", level);
            log!("Items size before", model.files_model.current_dir_path.len());
            model.files_model.current_dir_path.truncate(level);
            log!("Items size after", model.files_model.current_dir_path.len());
            orders.send_msg(Msg::ListItemClick(String::new()));
        }
        Msg::AddItemToQueue(id, is_dir) => {
            if is_dir {
                let mut current_dir = String::new();
                model.files_model.current_dir_path.iter().for_each(|p| {
                    current_dir.push_str(p);
                    current_dir.push('/');
                });
                current_dir.push_str(&id);
                current_dir.push('/');
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(AddLocalLibDirectory(
                    current_dir,
                ))));
            } else {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(AddSongToQueue(id))));
            }
        }
        Msg::LoadItemToQueue(id, is_dir) => {
            if is_dir {
                let mut current_dir = String::new();
                model.files_model.current_dir_path.iter().for_each(|p| {
                    current_dir.push_str(p);
                    current_dir.push('/');
                });
                current_dir.push_str(&id);
                current_dir.push('/');
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(LoadLocalLibDirectory(
                    current_dir,
                ))));
            } else {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(LoadSongToQueue(id))));
            }
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
    let mut level = 1;
    section![
        C!["section"],
        nav![
            C!["breadcrumb", "p-3"],
            attrs!(At::AriaLabel => "breadcrumbs"),
            IF!(!model.files_model.current_dir_path.is_empty() =>
            ul![
                C!["has-background-dark-transparent", "p-3"],
                li![a![
                    "Root",
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part("files")}
                ]],
                model.files_model.current_dir_path.iter().map(|dir| {
                    let l = li![a![dir, ev(Ev::Click, move |_| Msg::BreadcrumbClick(level))]];
                    level += 1;
                    l
                })
            ])
        ],
        div![
            IF!(model.wait_response => progress![C!["progress", "is-small"], attrs!{ At::Max => "100"}, style!{ St::MarginBottom => "50px"}]),
        ],
        div![model.files_model.items.iter().map(|item| {
            let title = item.get_title();
            let id = item.get_id();
            let id2 = item.get_id();
            let id3 = item.get_id();
            let is_dir = item.is_dir();
            div![
                C!["list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
                div![
                    C!["list-item"],
                    div![
                        C!["list-item-content", "has-background-dark-transparent"],
                        div![
                            C!["list-item-title", "has-text-light"],
                            span![&title],
                            if let MetadataLibraryItem::SongItem(song) = item {
                                span![
                                    &song.date.as_ref().map(|d| span![format!(" ({d})")]),
                                    &song
                                        .time
                                        .as_ref()
                                        .map(|t| span![format!(" [{}]", api_models::common::dur_to_string(t))])
                                ]
                            } else {
                                empty!()
                            }
                        ],
                        div![
                            C!["description", "has-text-light"],
                            if let MetadataLibraryItem::SongItem(song) = item {
                                span![
                                    song.artist.as_ref().map(|at| span![i!["Art: "], at]),
                                    song.album.as_ref().map(|a| span![i![" | Alb: "], a]),
                                    song.genre.as_ref().map(|a| p![i!["Genre: "], a]),
                                ]
                            } else {
                                empty!()
                            }
                        ],
                        IF!(is_dir => ev(Ev::Click, move |_| Msg::ListItemClick(id)))
                    ],
                    div![
                        C!["list-item-controls"],
                        div![
                            C!["buttons"],
                            a![
                                attrs!(At::Title =>"Add song to queue"),
                                C!["white-icon"],
                                i![C!("material-icons"), "playlist_add"],
                                ev(Ev::Click, move |_| Msg::AddItemToQueue(id2, is_dir))
                            ],
                            a![
                                attrs!(At::Title =>"Play song and replace queue"),
                                C!["white-icon"],
                                i![C!("material-icons"), "play_circle_filled"],
                                ev(Ev::Click, move |_| Msg::LoadItemToQueue(id3, is_dir))
                            ],
                        ]
                    ],
                ]
            ]
        })]
    ]
}

#[cfg(test)]
mod test {
    use gloo_console::log;
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
        let traverser = l1.traverse(arena);
        log!("<ul>");
        traverser.for_each(|n| match n {
            indextree::NodeEdge::Start(sn) => {
                let childen_count = sn.children(arena).count();
                let value = arena.get(sn).unwrap();
                log!(format!("<li>{}", value.get()));
                if childen_count > 0 {
                    log!(format!("<ul>"));
                }
            }
            indextree::NodeEdge::End(en) => {
                log!("</li>");
                if arena[en].next_sibling().is_none() {
                    log!("</ul>");
                }
            }
        });
        //https://jsfiddle.net/1fynun7a/1591/
        log!(format!("{}", l1.debug_pretty_print(arena)));
    }
}
