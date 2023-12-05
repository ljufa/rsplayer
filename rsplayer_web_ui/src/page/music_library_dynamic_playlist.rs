use std::collections::HashMap;

use api_models::common::PlaylistCommand::QueryDynamicPlaylists;
use api_models::common::UserCommand;
use api_models::playlist::DynamicPlaylistsPage;
use api_models::state::StateChangeEvent;

use api_models::{
    player::Song,
    playlist::{Category},
};
use gloo_console::log;
use gloo_net::http::Request;
use gloo_net::Error;
use seed::{
    a, attrs, button, div, figure, footer, header, i, id, img, nodes, p, prelude::*,
    progress, raw, section, span, style, C, IF,
};

use crate::{attachCarousel, scrollToId};

#[derive(Debug)]
pub struct Model {
    pub dynamic_playlists: HashMap<Category, DynamicPlaylistsPage>,
    pub playlist_categories: Vec<Category>,
    pub category_offset: usize,
    pub dynamic_playlist_loading: bool,
    pub selected_playlist_items: Vec<Song>,
    pub selected_playlist_id: String,
    pub selected_playlist_name: String,
    selected_playlist_current_page_no: usize,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    CategoriesFetched(Result<Vec<Category>, Error>),
    StatusChangeEventReceived(StateChangeEvent),
    SendUserCommand(UserCommand),
    ShowPlaylistItemsClicked(bool, String, String),
    LoadPlaylistIntoQueue(String),
    LoadAlbumQueue(String),
    CloseSelectedPlaylistItemsModal,
    KeyPressed(web_sys::KeyboardEvent),
    AddSongToQueue(String),
    PlaySongFromPlaylist(String),
    ShowCategoryPlaylists(String),
    LoadMoreCategories,
    LoadMorePlaylistItems,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.perform_cmd(async { Msg::CategoriesFetched(get_playlist_categories().await) });
    

    orders.stream(streams::window_event(Ev::KeyDown, |event| {
        Msg::KeyPressed(event.unchecked_into())
    }));
    Model {
        dynamic_playlist_loading: false,
        playlist_categories: Vec::default(),
        category_offset: 0,
        dynamic_playlists: HashMap::default(),
        selected_playlist_items: Vec::default(),
        selected_playlist_id: String::default(),
        selected_playlist_name: String::default(),
        selected_playlist_current_page_no: 1,
    }
}

// ------ ------
//    Update
// ------ ------
const DYNAMIC_PLAYLIST_PAGE_SIZE: u32 = 12;
const CATEGORY_PAGE_SIZE: usize = 10;

#[allow(clippy::too_many_lines)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    //log!("PL Update", msg);
    match msg {
        Msg::CategoriesFetched(Ok(categories)) => {
            model.playlist_categories = categories;
            orders.perform_cmd(async { Msg::LoadMoreCategories });
        }
        Msg::LoadMoreCategories => {
            let cat_ids: Vec<String> = model
                .playlist_categories
                .iter()
                .skip(model.category_offset)
                .take(CATEGORY_PAGE_SIZE)
                .map(|c| c.id.clone())
                .collect();

            model.category_offset += cat_ids.len();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                QueryDynamicPlaylists(cat_ids, 0, DYNAMIC_PLAYLIST_PAGE_SIZE),
            )));
            model.dynamic_playlist_loading = true;
            orders.after_next_render(move |_| scrollToId("dynamic-playlists-section"));
        }
        Msg::LoadMorePlaylistItems => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryPlaylistItems(
                    model.selected_playlist_id.clone(),
                    model.selected_playlist_current_page_no + 1,
                ),
            )));
            orders.after_next_render(move |_| scrollToId("first-list-item"));
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::DynamicPlaylistsPageEvent(
            dynamic_pls,
        )) => {
            model.dynamic_playlist_loading = false;
            if !dynamic_pls.is_empty() {
                model.dynamic_playlists.clear();
            }
            for dpl in &dynamic_pls {
                if let Some(cat) = model
                    .playlist_categories
                    .iter()
                    .find(|c| c.id == dpl.category_id)
                {
                    model.dynamic_playlists.insert(cat.clone(), dpl.clone());
                    let cid = cat.sanitized_id();
                    orders.after_next_render(move |_| {
                        attachCarousel(&format!("#cat-{cid}"));
                    });
                }
            }
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::PlaylistItemsEvent(
            playlist_items,
            page,
        )) => {
            model.selected_playlist_items = playlist_items;
            model.selected_playlist_current_page_no = page;
        }
        Msg::ShowPlaylistItemsClicked(_is_dynamic, playlist_id, playlist_name) => {
            model.selected_playlist_id = playlist_id.clone();
            model.selected_playlist_name = playlist_name;
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryPlaylistItems(playlist_id, 1),
            )));
        }
        Msg::CloseSelectedPlaylistItemsModal => {
            model.selected_playlist_items = Vec::default();
            model.selected_playlist_id = String::default();
            model.selected_playlist_name = String::default();
        }
        Msg::KeyPressed(event) => {
            if event.key() == "Escape" {
                model.selected_playlist_items = Vec::default();
                model.selected_playlist_id = String::default();
                model.selected_playlist_name = String::default();
            }
        }
        Msg::SendUserCommand(_cmd) => log!("Cmd:"),
        Msg::LoadPlaylistIntoQueue(pl_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadPlaylistInQueue(pl_id),
            )));
        }
        Msg::AddSongToQueue(song_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddSongToQueue(song_id),
            )));
        }
        Msg::PlaySongFromPlaylist(song_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadSongToQueue(song_id),
            )));
        }
        Msg::LoadAlbumQueue(album_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadAlbumInQueue(album_id),
            )));
        }
        _ => {}
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![
        view_selected_playlist_items_modal(model),
        view_dynamic_playlists(model),
    ]
}

fn view_selected_playlist_items_modal(model: &Model) -> Node<Msg> {
    let selected_playlist_id = model.selected_playlist_id.clone();
    div![
        C![
            "modal",
            IF!(!model.selected_playlist_items.is_empty() => "is-active")
        ],
        div![C!["modal-background"],
            ev(Ev::Click, |_| Msg::CloseSelectedPlaylistItemsModal),
        ],
        div![
            id!("selected-playlist-items-modal"),
            C!["modal-card"],
            header![C!["modal-card-head"],
                a![
                    attrs!(At::Title =>"Load playlist into queue"),
                    i![C!("is-large-icon material-icons"), "play_circle_filled"],
                    ev(Ev::Click, move |_| Msg::LoadPlaylistIntoQueue(selected_playlist_id))
                ],
                p![C!["modal-card-title"],style!(St::MarginLeft => "10px"), model.selected_playlist_name.clone()],
                button![C!["delete", "is-large"], attrs!(At::AriaLabel =>"close"), ev(Ev::Click, |_| Msg::CloseSelectedPlaylistItemsModal)],
            ],
            section![
                C!["modal-card-body"],
                div![C!["list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
                div![id!("first-list-item")],
                model.selected_playlist_items
                    .iter()
                    .map(|song| {
                        let song_id = song.get_identifier();
                        let song_id2 = song.get_identifier();
                        div![C!["list-item"],
                            div![
                                C!["list-item-content", "has-background-dark-transparent"],
                                div![
                                    C!["list-item-title", "has-text-light"],
                                    song.get_title()
                                ],
                                div![
                                    C!["description", "has-text-light"],
                                    song.artist.clone()
                                ]
                            ],
                            div![
                                C!["list-item-controls"],
                                div![
                                    C!["buttons"],
                                    a![
                                        attrs!(At::Title =>"Add song to queue"),
                                        C!["icon"], i![C!("material-icons"), "playlist_add"],
                                        ev(Ev::Click, move |_| Msg::AddSongToQueue(song_id))
                                    ],
                                    a![
                                        attrs!(At::Title =>"Play song and replace queue"),
                                        C!["icon"], i![C!("material-icons"), "play_circle_filled"],
                                        ev(Ev::Click, move |_| Msg::PlaySongFromPlaylist(song_id2))
                                    ],
                                ]
                            ],
                        ]
                    })
                ]
            ],
            footer![C!["modal-card-foot"],
                button![C!["button","is-fullwidth", "is-outlined", "is-success"],"Load more", ev(Ev::Click, move |_| Msg::LoadMorePlaylistItems)]

            ]
        ],
    ]
}

fn view_dynamic_playlists(model: &Model) -> Node<Msg> {
    section![
        div![id!("dynamic-playlists-section"),
            IF!(model.dynamic_playlist_loading => progress![C!["progress", "is-small"], attrs!{ At::Max => "100"}, style!{ St::MarginBottom => "50px"}]),
        ],
        C!["section"],
        div![
            C!["container"],
            model.dynamic_playlists.iter().map(|(category, page)|{
                let category_id2 = category.id.clone();
                nodes![
                    a![C!["title is-4 has-text-light has-background-dark-transparent"], 
                        category.name.clone(), raw!("&nbsp;"),
                        i![C!["material-icons"], "open_in_new"],
                        ev(Ev::Click, move |_| Msg::ShowCategoryPlaylists(category_id2))
                    ],
                    section![
                        C!["section"],
                        div![
                            C!["carousel"],
                            id!(format!("cat-{}", category.sanitized_id())),
                            page.playlists
                                .iter()
                                .map(|playlist|{
                                    let id = playlist.id.clone();
                                    let id2 = playlist.id.clone();
                                    let name = playlist.name.clone();
                                    div![
                                        C!["card"],
                                        div![
                                            C!["card-image"],
                                            figure![
                                                C!["image", "is-square"],
                                                img![
                                                    C!["image-center-half-size"],
                                                    attrs! {At::Src => playlist.image.as_ref().map_or("/headphones.png".to_string(),std::clone::Clone::clone)}
                                                ]
                                            ],
                                            span![
                                                C!["play-button"],
                                                ev(Ev::Click, move |_| Msg::LoadPlaylistIntoQueue(id))
                                            ]
                                        ],
                                        div![
                                            C!["card-content"],
                                            a![ ev(Ev::Click, |_| Msg::ShowPlaylistItemsClicked(true, id2, name)),
                                                C!["card-footer-item"],
                                                playlist.name.clone(),
                                                playlist.owner_name
                                                    .as_ref()
                                                    .map_or(String::new(), |ow| format!(" by {ow}"))
                                            ],
                                        ]
                                    ]
                                }),
                        ],
                    ]]}
                ),
            ],
            button![C!["button","is-fullwidth", "is-outlined", "is-success"],"Load more", ev(Ev::Click, move |_| Msg::LoadMoreCategories)]
    ]
}


#[allow(clippy::future_not_send)]
pub async fn get_playlist_categories() -> Result<Vec<Category>, Error> {
    Request::get("/api/categories")
        .send()
        .await?
        .json::<Vec<Category>>()
        .await
}