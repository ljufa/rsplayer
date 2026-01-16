use api_models::common::UserCommand;
use api_models::state::StateChangeEvent;
use api_models::{
    player::Song,
    playlist::{PlaylistType, Playlists},
};
use gloo_console::log;
use seed::{
    a, attrs, button, div, figure, footer, header, i, id, img, li, nodes, p, prelude::*, progress, section, span,
    style, ul, C, IF,
};

use crate::{attachCarousel, scrollToId};

#[derive(Debug)]
pub struct Model {
    pub static_playlists: Playlists,
    pub static_playlist_loading: bool,
    pub selected_playlist_items: Vec<Song>,
    pub selected_playlist_id: String,
    pub selected_playlist_name: String,
    pub selected_playlist_is_album: bool,
    selected_playlist_current_page_no: usize,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    StatusChangeEventReceived(StateChangeEvent),
    SendUserCommand(UserCommand),
    ShowPlaylistItemsClicked(bool, String, String),
    ShowAlbumItemsClicked(String),
    LoadPlaylistIntoQueue(String),
    LoadAlbumIntoQueue(String),
    AddPlaylistToQueue(String),
    AddAlbumToQueue(String),
    CloseSelectedPlaylistItemsModal,
    KeyPressed(web_sys::KeyboardEvent),
    AddSongToQueue(String),
    PlaySongFromPlaylist(String),
    LoadMorePlaylistItems,
    WebSocketOpen,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
        api_models::common::PlaylistCommand::QueryPlaylist,
    )));

    orders.stream(streams::window_event(Ev::KeyDown, |event| {
        Msg::KeyPressed(event.unchecked_into())
    }));
    Model {
        static_playlists: Playlists::default(),
        static_playlist_loading: true,
        selected_playlist_is_album: false,
        selected_playlist_items: Vec::default(),
        selected_playlist_id: String::default(),
        selected_playlist_name: String::default(),
        selected_playlist_current_page_no: 1,
    }
}

// ------ ------
//    Update
// ------ ------

#[allow(clippy::too_many_lines)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    //log!("PL Update", msg);
    match msg {
        Msg::LoadMorePlaylistItems => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryPlaylistItems(
                    model.selected_playlist_id.clone(),
                    model.selected_playlist_current_page_no + 1,
                ),
            )));
            orders.after_next_render(move |_| scrollToId("first-list-item"));
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::PlaylistItemsEvent(playlist_items, page)) => {
            model.selected_playlist_items = playlist_items;
            model.selected_playlist_current_page_no = page;
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::PlaylistsEvent(playlists)) => {
            model.static_playlist_loading = false;
            model.static_playlists = playlists;
            orders.after_next_render(|_| {
                attachCarousel("#featured-pl");
                attachCarousel("#saved-pl");
                attachCarousel("#newreleases-pl");
                attachCarousel("#favorites-pl");
            });
        }
        Msg::ShowPlaylistItemsClicked(_is_dynamic, playlist_id, playlist_name) => {
            model.selected_playlist_id.clone_from(&playlist_id);
            model.selected_playlist_name = playlist_name;
            model.selected_playlist_is_album = false;
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryPlaylistItems(playlist_id, 0),
            )));
        }
        Msg::ShowAlbumItemsClicked(album_id) => {
            model.selected_playlist_id.clone_from(&album_id);
            model.selected_playlist_name.clone_from(&album_id);
            model.selected_playlist_is_album = true;
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryAlbumItems(album_id, 0),
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
        Msg::LoadAlbumIntoQueue(album_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadAlbumInQueue(album_id),
            )));
        }
        Msg::AddAlbumToQueue(album_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddAlbumToQueue(album_id),
            )));
        }
        Msg::AddPlaylistToQueue(pl_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddPlaylistToQueue(pl_id),
            )));
        }
        Msg::WebSocketOpen => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                api_models::common::PlaylistCommand::QueryPlaylist,
            )));
        }
        _ => {}
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![view_selected_playlist_items_modal(model), view_static_playlists(model),]
}

fn view_selected_playlist_items_modal(model: &Model) -> Node<Msg> {
    let selected_playlist_id = model.selected_playlist_id.clone();
    let selected_playlist_id2 = model.selected_playlist_id.clone();
    div![
        C!["modal", IF!(!model.selected_playlist_items.is_empty() => "is-active")],
        div![
            C!["modal-background"],
            ev(Ev::Click, |_| Msg::CloseSelectedPlaylistItemsModal),
        ],
        div![
            id!("selected-playlist-items-modal"),
            C!["modal-card"],
            header![
                C!["modal-card-head"],
                a![
                    attrs!(At::Title =>"Load playlist into queue"),
                    i![C!("is-large-icon material-icons"), "play_circle_filled"],
                    if model.selected_playlist_is_album {
                        ev(Ev::Click, move |_| Msg::LoadAlbumIntoQueue(selected_playlist_id))
                    } else {
                        ev(Ev::Click, move |_| Msg::LoadPlaylistIntoQueue(selected_playlist_id))
                    }
                ],
                a![
                    attrs!(At::Title =>"Load playlist into queue"),
                    i![C!("is-large-icon material-icons"), "playlist_add"],
                    if model.selected_playlist_is_album {
                        ev(Ev::Click, move |_| Msg::AddAlbumToQueue(selected_playlist_id2))
                    } else {
                        ev(Ev::Click, move |_| Msg::AddPlaylistToQueue(selected_playlist_id2))
                    }
                ],
                p![
                    C!["modal-card-title"],
                    style! {
                        St::MarginLeft => "10px",
                        St::FlexShrink => "1"
                    },
                    model.selected_playlist_name.clone()
                ],
                button![
                    C!["delete", "is-large"],
                    attrs!(At::AriaLabel =>"close"),
                    ev(Ev::Click, |_| Msg::CloseSelectedPlaylistItemsModal)
                ],
            ],
            section![
                C!["modal-card-body"],
                div![
                    C!["list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
                    div![id!("first-list-item")],
                    model.selected_playlist_items.iter().map(|song| {
                        let song_id = song.file.clone();
                        let song_id2 = song.file.clone();
                        div![
                            C!["list-item"],
                            div![
                                C!["list-item-content", "has-background-dark-transparent"],
                                div![C!["list-item-title", "has-text-light"], song.get_title()],
                                div![C!["description", "has-text-light"], song.artist.clone()]
                            ],
                            div![
                                C!["list-item-controls"],
                                div![
                                    C!["buttons"],
                                    a![
                                        attrs!(At::Title =>"Add song to queue"),
                                        C!["icon"],
                                        i![C!("material-icons"), "playlist_add"],
                                        ev(Ev::Click, move |_| Msg::AddSongToQueue(song_id))
                                    ],
                                    a![
                                        attrs!(At::Title =>"Play song and replace queue"),
                                        C!["icon"],
                                        i![C!("material-icons"), "play_circle_filled"],
                                        ev(Ev::Click, move |_| Msg::PlaySongFromPlaylist(song_id2))
                                    ],
                                ]
                            ],
                        ]
                    })
                ]
            ],
            footer![
                C!["modal-card-foot"],
                button![
                    C!["button", "is-fullwidth", "is-outlined", "is-primary"],
                    "Load more",
                    ev(Ev::Click, move |_| Msg::LoadMorePlaylistItems)
                ]
            ]
        ],
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
            IF!(model.static_playlists.has_recently_added() => nodes![
                span![C!["title is-3 has-text-light has-background-dark-transparent"], "Recently added"],
                section![
                    C!["section"],
                    div![
                        C!["carousel"],
                        id!("featured-pl"),
                        model
                            .static_playlists
                            .items
                            .iter()
                            .filter(|it| it.is_recently_added())
                            .map(view_static_playlist_carousel_item)
                    ],
                ]]
            ),
            IF!(model.static_playlists.has_new_releases() => nodes![
            span![C!["title is-3 has-text-light has-background-dark-transparent"], "New releases"],
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
            IF!(model.static_playlists.has_saved() => nodes![
            span![C!["title is-3 has-text-light has-background-dark-transparent"], "Saved"],
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
            IF!(model.static_playlists.has_most_played() || model.static_playlists.has_liked() => nodes![
            span![C!["title is-3 has-text-light has-background-dark-transparent"], "Favorites"],
            section![
                C!["section"],
                div![
                    C!["carousel"],
                    id!("favorites-pl"),
                    model
                        .static_playlists
                        .items
                        .iter()
                        .filter(|it| it.is_most_played() || it.is_liked())
                        .map(view_static_playlist_carousel_item)
                ],
            ]]),
        ]
    ]
}

fn view_static_playlist_carousel_item(playlist: &PlaylistType) -> Node<Msg> {
    match playlist {
        PlaylistType::Featured(pl)
        | PlaylistType::Saved(pl)
        | PlaylistType::MostPlayed(pl)
        | PlaylistType::Liked(pl) => {
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
                                C!["image-center-half-size"],
                                attrs! {At::Src => pl.image.as_ref().map_or("/headphones.png".to_string(),std::clone::Clone::clone)}
                            ]
                        ],
                        span![C!["play-button"], ev(Ev::Click, |_| Msg::LoadPlaylistIntoQueue(id))]
                    ],
                    div![a![
                        C!["card-footer-item", "box"],
                        ev(Ev::Click, |_| Msg::ShowPlaylistItemsClicked(false, id2, name)),
                        C!["card-footer-item"],
                        pl.name.clone()
                    ],]
                ]
            ]
        }
        PlaylistType::LatestRelease(album) | PlaylistType::RecentlyAdded(album) => {
            let id = album.title.clone();
            let id2 = album.title.clone();
            div![
                C![format!("item-{id}")],
                div![
                    C!["card"],
                    div![
                        C!["card-image"],
                        figure![
                            C!["image", "is-square"],
                            img![
                                C!["image-center-half-size"],
                                IF!(album.image_id.is_none() => attrs! {At::Src => "/headphones.png"}),
                                IF!(album.image_id.is_some() => attrs! {At::Src => format!("/artwork/{}", album.image_id.as_ref().unwrap())}),
                            ]
                        ],
                        span![C!["play-button"], ev(Ev::Click, |_| Msg::LoadAlbumIntoQueue(id))]
                    ],
                    div![a![
                        ev(Ev::Click, |_| Msg::ShowAlbumItemsClicked(id2)),
                        C!["card-footer-item", "box"],
                        ul![
                            style! {St::TextAlign => "center"},
                            li![i![album.title.clone()]],
                            li![album.artist.as_ref().map_or(String::new(), |art| art.clone())],
                            li![album.genre.as_ref().map_or(String::new(), |genre| genre.clone())],
                            li![album
                                .released
                                .as_ref()
                                .map_or(String::new(), |rdate| format!("{}", rdate.format("%Y")))],
                        ]
                    ],]
                ]
            ]
        }
    }
}
