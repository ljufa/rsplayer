use std::collections::HashSet;

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
    expanded_sections: HashSet<String>,
    carousel_attached: HashSet<String>,
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
    AddPlaylistAfterCurrent(String),
    AddAlbumAfterCurrent(String),
    CloseSelectedPlaylistItemsModal,
    KeyPressed(web_sys::KeyboardEvent),
    AddSongToQueue(String),
    AddSongAfterCurrent(String),
    AddSongAndPlay(String),
    PlaySongFromPlaylist(String),
    LoadMorePlaylistItems,
    ToggleSection(String),
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
    let default_expanded: HashSet<String> = [
        "featured-pl".to_string(),
        "newreleases-pl".to_string(),
        "saved-pl".to_string(),
        "favorites-pl".to_string(),
    ]
    .into_iter()
    .collect();
    Model {
        static_playlists: Playlists::default(),
        static_playlist_loading: true,
        selected_playlist_is_album: false,
        selected_playlist_items: Vec::default(),
        selected_playlist_id: String::default(),
        selected_playlist_name: String::default(),
        selected_playlist_current_page_no: 1,
        expanded_sections: default_expanded,
        carousel_attached: HashSet::new(),
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
            // Only attach carousels for the default-expanded sections
            let to_attach: Vec<String> = model
                .expanded_sections
                .iter()
                .filter(|id| !model.carousel_attached.contains(*id))
                .cloned()
                .collect();
            for id in &to_attach {
                model.carousel_attached.insert(id.clone());
            }
            orders.after_next_render(move |_| {
                for id in &to_attach {
                    attachCarousel(&format!("#{id}"));
                }
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
        Msg::AddSongAfterCurrent(song_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddSongAfterCurrent(song_id),
            )));
        }
        Msg::AddSongAndPlay(song_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddSongAndPlay(song_id),
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
        Msg::AddPlaylistAfterCurrent(pl_id) => {
            // Load items then add each after current — reuse LoadPlaylistInQueue for simplicity
            // but with the dedicated command once available; for now use AddPlaylistToQueue
            // (a full "insert after" for playlists would need a batch backend command)
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddPlaylistToQueue(pl_id),
            )));
        }
        Msg::AddAlbumAfterCurrent(album_id) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddAlbumToQueue(album_id),
            )));
        }
        Msg::ToggleSection(section_id) => {
            if model.expanded_sections.contains(&section_id) {
                model.expanded_sections.remove(&section_id);
            } else {
                model.expanded_sections.insert(section_id.clone());
                if !model.carousel_attached.contains(&section_id) {
                    model.carousel_attached.insert(section_id.clone());
                    orders.after_next_render(move |_| {
                        attachCarousel(&format!("#{section_id}"));
                    });
                }
            }
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
    let selected_playlist_id3 = model.selected_playlist_id.clone();
    let is_album = model.selected_playlist_is_album;
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
                    attrs!(At::Title =>"Replace queue & play"),
                    i![C!("is-large-icon material-icons"), "play_circle_filled"],
                    if is_album {
                        ev(Ev::Click, move |_| Msg::LoadAlbumIntoQueue(selected_playlist_id))
                    } else {
                        ev(Ev::Click, move |_| Msg::LoadPlaylistIntoQueue(selected_playlist_id))
                    }
                ],
                a![
                    attrs!(At::Title =>"Add to queue"),
                    i![C!("is-large-icon material-icons"), "playlist_add"],
                    if is_album {
                        ev(Ev::Click, move |_| Msg::AddAlbumToQueue(selected_playlist_id2))
                    } else {
                        ev(Ev::Click, move |_| Msg::AddPlaylistToQueue(selected_playlist_id2))
                    }
                ],
                a![
                    attrs!(At::Title =>"Play Next (add after current)"),
                    i![C!("is-large-icon material-icons"), "playlist_play"],
                    if is_album {
                        ev(Ev::Click, move |_| Msg::AddAlbumAfterCurrent(selected_playlist_id3))
                    } else {
                        ev(Ev::Click, move |_| Msg::AddPlaylistAfterCurrent(selected_playlist_id3))
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
                        let song_id_next = song.file.clone();
                        let song_id_play = song.file.clone();
                        let song_id_replace = song.file.clone();
                        div![
                            C!["list-item"],
                            div![
                                C!["list-item-content"],
                                div![C!["list-item-title", "has-text-light"], song.get_title()],
                                div![C!["description", "has-text-light"], song.artist.clone()]
                            ],
                            div![
                                C!["list-item-controls"],
                                div![
                                    C!["song-actions"],
                                    i![C!["material-icons", "song-actions__trigger"], "more_vert"],
                                    div![
                                        C!["song-actions__btns"],
                                        a![
                                            attrs!(At::Title =>"Add to queue"),
                                            C!["icon", "white-icon"],
                                            i![C!("material-icons"), "playlist_add"],
                                            ev(Ev::Click, move |_| Msg::AddSongToQueue(song_id))
                                        ],
                                        a![
                                            attrs!(At::Title =>"Play Next"),
                                            C!["icon", "white-icon"],
                                            i![C!("material-icons"), "playlist_play"],
                                            ev(Ev::Click, move |_| Msg::AddSongAfterCurrent(song_id_next))
                                        ],
                                        a![
                                            attrs!(At::Title =>"Play Now"),
                                            C!["icon", "white-icon"],
                                            i![C!("material-icons"), "play_arrow"],
                                            ev(Ev::Click, move |_| Msg::AddSongAndPlay(song_id_play))
                                        ],
                                        a![
                                            attrs!(At::Title =>"Replace queue & play"),
                                            C!["icon", "white-icon"],
                                            i![C!("material-icons"), "play_circle_filled"],
                                            ev(Ev::Click, move |_| Msg::PlaySongFromPlaylist(song_id_replace))
                                        ],
                                    ]
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
            // -- Recently added --
            IF!(model.static_playlists.has_recently_added() =>
                view_collapsible_section(model, "featured-pl", "Recently added", &model
                    .static_playlists
                    .items
                    .iter()
                    .filter(|it| it.is_recently_added())
                    .collect::<Vec<_>>())
            ),
            // -- New releases --
            IF!(model.static_playlists.has_new_releases() =>
                view_collapsible_section(model, "newreleases-pl", "New releases", &model
                    .static_playlists
                    .items
                    .iter()
                    .filter(|it| it.is_new_release())
                    .collect::<Vec<_>>())
            ),
            // -- Saved --
            IF!(model.static_playlists.has_saved() =>
                view_collapsible_section(model, "saved-pl", "Saved", &model
                    .static_playlists
                    .items
                    .iter()
                    .filter(|it| it.is_saved())
                    .collect::<Vec<_>>())
            ),
            // -- Favorites --
            IF!(model.static_playlists.has_most_played() || model.static_playlists.has_liked() =>
                view_collapsible_section(model, "favorites-pl", "Favorites", &model
                    .static_playlists
                    .items
                    .iter()
                    .filter(|it| it.is_most_played() || it.is_liked())
                    .collect::<Vec<_>>())
            ),
            // -- By Genre --
            IF!(model.static_playlists.has_by_genre() =>
                nodes![model.static_playlists.genres().into_iter().map(|genre| {
                    let carousel_id = format!("genre-pl-{}", pl_slug(&genre));
                    let items: Vec<&PlaylistType> = model
                        .static_playlists
                        .items
                        .iter()
                        .filter(|it| {
                            if let PlaylistType::ByGenre(a) = it {
                                a.genre.as_deref() == Some(genre.as_str())
                            } else {
                                false
                            }
                        })
                        .collect();
                    view_collapsible_section(model, &carousel_id, &genre, &items)
                }).collect::<Vec<_>>()]
            ),
            // -- By Decade --
            IF!(model.static_playlists.has_by_decade() =>
                nodes![model.static_playlists.decades().into_iter().map(|decade| {
                    let carousel_id = format!("decade-pl-{}", pl_slug(&decade));
                    let items: Vec<&PlaylistType> = model
                        .static_playlists
                        .items
                        .iter()
                        .filter(|it| {
                            if let PlaylistType::ByDecade(a) = it {
                                a.released.map_or(false, |r| {
                                    let year = r.format("%Y").to_string();
                                    year.len() == 4 && format!("{}0s", &year[..3]) == decade
                                })
                            } else {
                                false
                            }
                        })
                        .collect();
                    view_collapsible_section(model, &carousel_id, &decade, &items)
                }).collect::<Vec<_>>()]
            ),
        ]
    ]
}

fn view_collapsible_section(model: &Model, carousel_id: &str, title: &str, items: &[&PlaylistType]) -> Node<Msg> {
    let is_expanded = model.expanded_sections.contains(carousel_id);
    let toggle_id = carousel_id.to_string();
    let icon = if is_expanded { "expand_less" } else { "expand_more" };
    let item_count = items.len();
    div![
        div![
            style! {
                St::Cursor => "pointer",
                St::Display => "flex",
                St::AlignItems => "center",
                St::UserSelect => "none",
                St::MarginBottom => "4px",
            },
            ev(Ev::Click, move |_| Msg::ToggleSection(toggle_id)),
            i![
                C!["material-icons", "has-text-light"],
                style! { St::FontSize => "28px", St::MarginRight => "8px" },
                icon,
            ],
            span![
                C!["title is-3 has-text-light has-background-dark-transparent"],
                style! { St::MarginBottom => "0" },
                title,
            ],
            span![
                C!["has-text-grey-light"],
                style! { St::MarginLeft => "10px", St::FontSize => "0.9rem" },
                format!("({item_count})"),
            ],
        ],
        IF!(is_expanded => section![
            C!["section"],
            div![
                C!["carousel"],
                id!(carousel_id.to_string()),
                items.iter().map(|it| view_static_playlist_carousel_item(it))
            ],
        ]),
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
            let id3 = pl.id.clone();
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
                                attrs! {At::Src => pl.image.as_ref().map_or("/no_album.svg".to_string(),std::clone::Clone::clone)}
                            ]
                        ],
                        span![
                            C!["play-button-small"],
                            attrs! {At::Title => "Load playlist into queue"},
                            ev(Ev::Click, move |_| Msg::LoadPlaylistIntoQueue(id))
                        ],
                        span![
                            C!["add-button-small"],
                            attrs! {At::Title => "Add playlist to queue"},
                            ev(Ev::Click, move |_| Msg::AddPlaylistToQueue(id2))
                        ]
                    ],
                    div![a![
                        C!["card-footer-item", "box"],
                        ev(Ev::Click, move |_| Msg::ShowPlaylistItemsClicked(false, id3, name)),
                        C!["card-footer-item"],
                        pl.name.clone()
                    ],]
                ]
            ]
        }
        PlaylistType::LatestRelease(album)
        | PlaylistType::RecentlyAdded(album)
        | PlaylistType::ByGenre(album)
        | PlaylistType::ByDecade(album) => {
            let id = album.title.clone();
            let id2 = album.title.clone();
            let id3 = album.title.clone();
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
                                IF!(album.image_id.is_none() => attrs! {At::Src => "/no_album.svg"}),
                                IF!(album.image_id.is_some() => attrs! {At::Src => format!("/artwork/{}", album.image_id.as_ref().unwrap())}),
                            ]
                        ],
                        span![
                            C!["play-button-small"],
                            attrs! {At::Title => "Load album into queue"},
                            ev(Ev::Click, move |_| Msg::LoadAlbumIntoQueue(id))
                        ],
                        span![
                            C!["add-button-small"],
                            attrs! {At::Title => "Add album to queue"},
                            ev(Ev::Click, move |_| Msg::AddAlbumToQueue(id2))
                        ]
                    ],
                    div![a![
                        ev(Ev::Click, move |_| Msg::ShowAlbumItemsClicked(id3)),
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

/// Convert a label (genre name, decade string, etc.) into a safe CSS id fragment.
fn pl_slug(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
