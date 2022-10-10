use std::collections::HashMap;

use api_models::playlist::DynamicPlaylistsPage;
use api_models::state::StateChangeEvent;

use api_models::{
    common::PlayerCommand,
    player::*,
    playlist::{Category, PlaylistType, Playlists},
};
use seed::{prelude::*, *};

use crate::{attachCarousel, scrollToId};

#[derive(Debug)]
pub struct Model {
    pub static_playlists: Playlists,
    pub static_playlist_loading: bool,
    pub dynamic_playlists: HashMap<Category, DynamicPlaylistsPage>,
    pub playlist_categories: Vec<Category>,
    pub category_offset: usize,
    pub dynamic_playlist_loading: bool,
    pub selected_playlist_items: Vec<Song>,
    pub selected_playlist_id: String,
    pub selected_playlist_name: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    StaticPlaylistsFetched(fetch::Result<Playlists>),
    CategoriesFetched(fetch::Result<Vec<Category>>),
    StatusChangeEventReceived(StateChangeEvent),
    SendCommand(PlayerCommand),
    ShowPlaylistItemsClicked(bool, String, String),
    LoadPlaylistIntoQueue(String),
    LoadAlbumQueue(String),
    CloseSelectedPlaylistItemsModal,
    KeyPressed(web_sys::KeyboardEvent),
    AddSongToQueue(String),
    PlaySongFromPlaylist(String),
    ShowCategoryPlaylists(String),
    LoadMoreCategories,
}

pub(crate) fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.perform_cmd(async { Msg::StaticPlaylistsFetched(get_playlists().await) });
    orders.perform_cmd(async { Msg::CategoriesFetched(get_playlist_categories().await) });
    orders.stream(streams::window_event(Ev::KeyDown, |event| {
        Msg::KeyPressed(event.unchecked_into())
    }));
    Model {
        dynamic_playlist_loading: false,
        static_playlists: Playlists::default(),
        static_playlist_loading: true,
        playlist_categories: Default::default(),
        category_offset: 0,
        dynamic_playlists: Default::default(),
        selected_playlist_items: Default::default(),
        selected_playlist_id: Default::default(),
        selected_playlist_name: Default::default(),
    }
}

// ------ ------
//    Update
// ------ ------
const DYNAMIC_PLAYLIST_PAGE_SIZE: u32 = 12;
const CATEGORY_PAGE_SIZE: usize = 10;

pub(crate) fn update(msg: Msg, mut model: &mut Model, orders: &mut impl Orders<Msg>) {
    //log!("PL Update", msg);
    match msg {
        Msg::StaticPlaylistsFetched(pls) => {
            model.static_playlist_loading = false;
            model.static_playlists = pls.unwrap_or_default();
            orders.after_next_render(|_| {
                attachCarousel("#featured-pl");
                attachCarousel("#saved-pl");
                attachCarousel("#newreleases-pl");
            });
        }
        Msg::CategoriesFetched(Ok(categories)) => {
            model.playlist_categories = categories;
        }
        Msg::LoadMoreCategories => {
            let cat_ids: Vec<String> = model
                .playlist_categories
                .iter()
                .skip(model.category_offset)
                .take(CATEGORY_PAGE_SIZE)
                .map(|c| c.id.clone())
                .collect();
            log!("Cat ids", cat_ids);
            model.category_offset += cat_ids.len();
            orders.send_msg(Msg::SendCommand(PlayerCommand::QueryDynamicPlaylists(
                cat_ids,
                0,
                DYNAMIC_PLAYLIST_PAGE_SIZE,
            )));
            model.dynamic_playlist_loading = true;
            orders.after_next_render(move |_| scrollToId("dynamic-playlists-section"));
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::DynamicPlaylistsPageEvent(
            dynamic_pls,
        )) => {
            model.dynamic_playlist_loading = false;
            if !dynamic_pls.is_empty() {
                model.dynamic_playlists.clear();
            }
            dynamic_pls.iter().for_each(|dpl| {
                if let Some(cat) = model
                    .playlist_categories
                    .iter()
                    .find(|c| c.id == dpl.category_id)
                {
                    if dpl.playlists.len() >= 10 {
                        model.dynamic_playlists.insert(cat.clone(), dpl.clone());
                        let cid = cat.sanitized_id();
                        orders.after_next_render(move |_| {
                            attachCarousel(&format!("#cat-{}", cid));
                        });
                    }
                }
            });
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::PlaylistItemsEvent(playlist_items)) => {
            model.selected_playlist_items = playlist_items;
        }
        Msg::ShowPlaylistItemsClicked(_is_dynamic, playlist_id, playlist_name) => {
           model.selected_playlist_id = playlist_id.clone();
            model.selected_playlist_name = playlist_name;
            orders.send_msg(Msg::SendCommand(PlayerCommand::QueryPlaylistItems(
                playlist_id,
            )));
        }
        Msg::CloseSelectedPlaylistItemsModal => {
            model.selected_playlist_items = Default::default();
            model.selected_playlist_id = Default::default();
            model.selected_playlist_name = Default::default();
        }
        Msg::KeyPressed(event) => {
            if event.key() == "Escape" {
                model.selected_playlist_items = Default::default();
                model.selected_playlist_id = Default::default();
                model.selected_playlist_name = Default::default();
            }
        }
        Msg::SendCommand(cmd) => log!("Cmd:", cmd),
        Msg::LoadPlaylistIntoQueue(pl_id) => {
            orders.send_msg(Msg::SendCommand(PlayerCommand::LoadPlaylist(pl_id)));
        }
        Msg::AddSongToQueue(song_id) => {
            orders.send_msg(Msg::SendCommand(PlayerCommand::AddSongToQueue(song_id)));
        }
        Msg::PlaySongFromPlaylist(song_id) => {
            orders.send_msg(Msg::SendCommand(PlayerCommand::LoadSong(song_id)));
        }
        Msg::LoadAlbumQueue(album_id) => {
            orders.send_msg(Msg::SendCommand(PlayerCommand::LoadAlbum(album_id)));
        }
        _ => {}
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![
        view_selected_playlist_items_modal(model),
        view_static_playlists(model),
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
                p![C!["modal-card-title"],style!(St::MarginLeft => "20px"), "Playlist - ", model.selected_playlist_name.clone()],
                button![C!["delete", "is-large"], attrs!(At::AriaLabel =>"close"), ev(Ev::Click, |_| Msg::CloseSelectedPlaylistItemsModal)],
            ],
            section![
                C!["modal-card-body"], style!{ St::Overflow => "Hidden", St::Padding => "0px" },
                div![C!["scroll-list list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
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
                    a![C!["title is-4"], 
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
                                                    attrs! {At::Src => playlist.image.as_ref().map_or("/no_album.png".to_string(),|i| i.clone())}
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
                                                    .map_or("".to_string(), |ow| format!(" by {ow}"))
                                            ],
                                        ]
                                    ]
                                }),
                        ],
                    ]]}
                ),
            ],
            button![C!["button","is-fullwidth", "is-outlined", "is-dark"],"Load more", ev(Ev::Click, move |_| Msg::LoadMoreCategories)]
    ]
}

fn view_static_playlists(model: &Model) -> Node<Msg> {
    section![
        div![
            IF!(model.static_playlist_loading => progress![C!["progress", "is-small"], attrs!{ At::Max => "100"}, style!{ St::MarginBottom => "50px"}]),
        ],

        C!["section"],
        div![
            C!["container"],
            IF!(model.static_playlists.has_featured() => nodes![
                span![C!["title is-3"], "Featured"],
                section![
                    C!["section"],
                    div![
                        C!["carousel"],
                        id!("featured-pl"),
                        model
                            .static_playlists
                            .items
                            .iter()
                            .filter(|it| it.is_featured())
                            .map(view_static_playlist_carousel_item)
                    ],
                ]]
            ),
            IF!(model.static_playlists.has_saved() => nodes![
            span![C!["title is-3"], "Saved"],
            section![
                C!["section"],
                div![
                    C!["carousel"],
                    id!("saved-pl"),
                    model
                        .static_playlists
                        .items
                        .iter()
                        .filter(|it| it.is_saved())
                        .map(view_static_playlist_carousel_item)
                ],
            ]]),
            IF!(model.static_playlists.has_new_releases() => nodes![
            span![C!["title is-3"], "New releases"],
            section![
                C!["section"],
                div![
                    C!["carousel"],
                    id!("newreleases-pl"),
                    model
                        .static_playlists
                        .items
                        .iter()
                        .filter(|it| it.is_new_release())
                        .map(view_static_playlist_carousel_item)
                ],
            ]]),
        ]
    ]
}

fn view_static_playlist_carousel_item(playlist: &PlaylistType) -> Node<Msg> {
    match playlist {
        PlaylistType::Featured(pl) | PlaylistType::Saved(pl) => {
            let id = pl.id.clone();
            let id2 = pl.id.clone();
            let name = pl.name.clone();
            div![
                C![format!("item-{id}")],
                div![
                    C!["card"],
                    div![
                        C!["card-image"],
                        figure![
                            C!["image", "is-square"],
                            img![
                                attrs! {At::Src => pl.image.as_ref().map_or("/no_album.png".to_string(),|i| i.clone())}
                            ]
                        ],
                        span![
                            C!["play-button"],
                            ev(Ev::Click, |_| Msg::LoadPlaylistIntoQueue(id))
                        ]
                    ],
                    div![
                        C!["card-footer"],
                        a![
                            ev(Ev::Click, |_| Msg::ShowPlaylistItemsClicked(
                                false, id2, name
                            )),
                            C!["card-footer-item"],
                            pl.name.clone(),
                            pl.owner_name
                                .as_ref()
                                .map_or("".to_string(), |ow| format!(" by {ow}"))
                        ],
                    ]
                ]
            ]
        }
        PlaylistType::NewRelease(album) => {
            let id = album.id.clone();
            div![
                C![format!("item-{id}")],
                div![
                    C!["card"],
                    div![
                        C!["card-image"],
                        figure![
                            C!["image", "is-square"],
                            img![
                                attrs! {At::Src => album.images.first().map_or("/no_album.png".to_string(),|i| i.clone())}
                            ]
                        ],
                        span![
                            C!["play-button"],
                            ev(Ev::Click, |_| Msg::LoadAlbumQueue(id))
                        ]
                    ],
                    div![
                        C!["card-content"],
                        //C!["card-footer-item"],
                        p![format!("Album: {}", album.album_name.clone())],
                        album
                            .artists
                            .first()
                            .map_or(empty!(), |art| p![format!("Artist: {art}")]),
                        album
                            .release_date
                            .as_ref()
                            .map_or(empty!(), |rdate| p![format!("Release date: {rdate}")])
                    ]
                ]
            ]
        }
    }
}

pub async fn get_playlists() -> fetch::Result<Playlists> {
    Request::new("/api/playlist")
        .method(Method::Get)
        .fetch()
        .await?
        .check_status()?
        .json::<Playlists>()
        .await
}

pub async fn get_playlist_categories() -> fetch::Result<Vec<Category>> {
    Request::new("/api/categories")
        .method(Method::Get)
        .fetch()
        .await?
        .check_status()?
        .json::<Vec<Category>>()
        .await
}
