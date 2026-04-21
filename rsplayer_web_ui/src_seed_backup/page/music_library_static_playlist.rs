use std::collections::{HashMap, HashSet};

use api_models::common::UserCommand;
use api_models::playlist::Album;
use api_models::state::StateChangeEvent;
use api_models::{
    player::Song,
    playlist::{PlaylistType, Playlists},
};
use gloo_console::log;
use seed::{
    a, attrs, button, div, figure, footer, h3, header, i, id, img, li, nav, nodes, p, prelude::*, progress, section,
    span, style, ul, C, IF,
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
    /// Lazily loaded albums keyed by carousel_id (e.g. "genre-pl-rock", "decade-pl-1990s")
    lazy_albums: HashMap<String, Vec<Album>>,
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
    LoadGenreInQueue(String),
    AddGenreToQueue(String),
    LoadDecadeInQueue(String),
    AddDecadeToQueue(String),
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
        lazy_albums: HashMap::new(),
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
            let to_attach: Vec<String> = model.expanded_sections.iter().cloned().collect();
            orders.after_next_render(move |_| {
                for id in &to_attach {
                    attachCarousel(&format!("#{id}"));
                }
            });
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::GenreAlbumsEvent(genre, albums)) => {
            let carousel_id = format!("genre-pl-{}", pl_slug(&genre));
            model.lazy_albums.insert(carousel_id.clone(), albums);
            orders.after_next_render(move |_| {
                attachCarousel(&format!("#{carousel_id}"));
            });
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::DecadeAlbumsEvent(decade, albums)) => {
            let carousel_id = format!("decade-pl-{}", pl_slug(&decade));
            model.lazy_albums.insert(carousel_id.clone(), albums);
            orders.after_next_render(move |_| {
                attachCarousel(&format!("#{carousel_id}"));
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

                // For genre/decade sections, lazy-load album data if not yet fetched.
                // The response handler will attach the carousel when data arrives.
                let needs_fetch = if !model.lazy_albums.contains_key(&section_id) {
                    if let Some(genre) = section_id.strip_prefix("genre-pl-") {
                        if let Some((name, _)) = model
                            .static_playlists
                            .genre_headers()
                            .into_iter()
                            .find(|(g, _)| pl_slug(g) == genre)
                        {
                            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                                api_models::common::PlaylistCommand::QueryAlbumsByGenre(name),
                            )));
                            true
                        } else {
                            false
                        }
                    } else if let Some(decade) = section_id.strip_prefix("decade-pl-") {
                        if let Some((name, _)) = model
                            .static_playlists
                            .decade_headers()
                            .into_iter()
                            .find(|(d, _)| pl_slug(d) == decade)
                        {
                            orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                                api_models::common::PlaylistCommand::QueryAlbumsByDecade(name),
                            )));
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Re-attach carousel on the fresh DOM element (unless we're
                // waiting for a fetch — the response handler will attach it).
                if !needs_fetch {
                    orders.after_next_render(move |_| {
                        attachCarousel(&format!("#{section_id}"));
                    });
                }
            }
        }
        Msg::LoadGenreInQueue(genre) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadGenreInQueue(genre),
            )));
        }
        Msg::AddGenreToQueue(genre) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddGenreToQueue(genre),
            )));
        }
        Msg::LoadDecadeInQueue(decade) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::LoadDecadeInQueue(decade),
            )));
        }
        Msg::AddDecadeToQueue(decade) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::AddDecadeToQueue(decade),
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
    div![
        view_breadcrumbs(model),
        view_selected_playlist_items_modal(model),
        view_static_playlists(model),
    ]
}

fn view_breadcrumbs(_model: &Model) -> Node<Msg> {
    nav![
        C!["breadcrumb-nav"],
        a![
            C!["breadcrumb-nav__item"],
            attrs! { At::Href => "#/library/playlists" },
            i![C!["material-icons", "breadcrumb-nav__icon"], "home"],
            span!["Library"],
        ],
        span![C!["breadcrumb-nav__separator"], "/"],
        span![
            C!["breadcrumb-nav__item", "is-current"],
            i![C!["material-icons", "breadcrumb-nav__icon"], "playlist_play"],
            span!["Playlists"],
        ],
    ]
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
    // Check if playlists are completely empty
    let has_any_playlists = model.static_playlists.items.iter().any(|it| {
        it.is_recently_added() || it.is_new_release() || it.is_saved() || it.is_most_played() || it.is_liked()
    });

    let has_genre_decade = model.static_playlists.has_genre_headers() || model.static_playlists.has_decade_headers();

    let is_empty = !model.static_playlist_loading && !has_any_playlists && !has_genre_decade;

    if is_empty {
        return view_empty_playlists();
    }

    // Show skeleton while loading initially
    if model.static_playlist_loading && model.static_playlists.items.is_empty() {
        return view_skeleton_playlists();
    }

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
            // -- By Genre (lazy loaded) --
            IF!(model.static_playlists.has_genre_headers() =>
                nodes![model.static_playlists.genre_headers().into_iter().map(|(genre, count)| {
                    let carousel_id = format!("genre-pl-{}", pl_slug(&genre));
                    view_lazy_section(model, &carousel_id, &genre, count, CategoryKind::Genre)
                }).collect::<Vec<_>>()]
            ),
            // -- By Decade (lazy loaded) --
            IF!(model.static_playlists.has_decade_headers() =>
                nodes![model.static_playlists.decade_headers().into_iter().map(|(decade, count)| {
                    let carousel_id = format!("decade-pl-{}", pl_slug(&decade));
                    view_lazy_section(model, &carousel_id, &decade, count, CategoryKind::Decade)
                }).collect::<Vec<_>>()]
            ),
        ]
    ]
}

fn view_empty_playlists() -> Node<Msg> {
    div![
        C!["empty-state"],
        i![C!["material-icons", "empty-state__icon"], "queue_music"],
        h3![C!["empty-state__title"], "No playlists yet"],
        p![C!["empty-state__description"], 
            "You don't have any playlists yet. Start listening to music and your playlists will appear here automatically."],
        div![
            C!["empty-state__actions"],
            a![
                C!["empty-state__cta"],
                attrs! { At::Href => "#/library/files" },
                i![C!["material-icons"], "library_music"],
                "Browse Library",
            ],
            button![
                C!["empty-state__secondary"],
                i![C!["material-icons"], "refresh"],
                "Refresh",
                ev(Ev::Click, |_| Msg::SendUserCommand(UserCommand::Playlist(
                    api_models::common::PlaylistCommand::QueryPlaylist
                )))
            ],
        ],
    ]
}

fn view_skeleton_playlists() -> Node<Msg> {
    div![
        C!["skeleton-container"],
        style! { St::Padding => "20px" },
        // Section title skeleton
        div![
            C!["skeleton skeleton--title"],
            style! { St::Width => "200px", St::MarginBottom => "20px" },
        ],
        // Carousel skeleton with 4 items
        div![
            C!["skeleton-carousel"],
            (0..4).map(|_| {
                div![
                    C!["skeleton-carousel-item"],
                    div![C!["skeleton skeleton-carousel-image"]],
                    div![C!["skeleton skeleton-carousel-title"]],
                ]
            })
        ],
        // Another section
        div![
            C!["skeleton skeleton--title"],
            style! { St::Width => "150px", St::MarginTop => "30px", St::MarginBottom => "20px" },
        ],
        div![
            C!["skeleton-carousel"],
            (0..4).map(|_| {
                div![
                    C!["skeleton-carousel-item"],
                    div![C!["skeleton skeleton-carousel-image"]],
                    div![C!["skeleton skeleton-carousel-title"]],
                ]
            })
        ],
    ]
}

fn view_collapsible_section(model: &Model, carousel_id: &str, title: &str, items: &[&PlaylistType]) -> Node<Msg> {
    let is_expanded = model.expanded_sections.contains(carousel_id);
    let toggle_id = carousel_id.to_string();
    let icon = if is_expanded { "expand_less" } else { "expand_more" };
    let item_count = items.len();
    div![
        div![
            C!["has-background-dark-transparent"],
            style! {
                St::Cursor => "pointer",
                St::Display => "flex",
                St::AlignItems => "center",
                St::UserSelect => "none",
                St::MarginBottom => "4px",
                St::Width => "100%",
                St::Padding => "2px 8px",
            },
            ev(Ev::Click, move |_| Msg::ToggleSection(toggle_id)),
            i![
                C!["material-icons", "has-text-light"],
                style! { St::FontSize => "28px", St::MarginRight => "8px" },
                icon,
            ],
            span![
                C!["title is-5 has-text-light"],
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

#[derive(Clone, Copy)]
enum CategoryKind {
    Genre,
    Decade,
}

fn view_lazy_section(
    model: &Model,
    carousel_id: &str,
    title: &str,
    header_count: usize,
    kind: CategoryKind,
) -> Node<Msg> {
    let is_expanded = model.expanded_sections.contains(carousel_id);
    let toggle_id = carousel_id.to_string();
    let icon = if is_expanded { "expand_less" } else { "expand_more" };
    let albums = model.lazy_albums.get(carousel_id);
    let item_count = albums.map_or(header_count, Vec::len);
    let title_for_load = title.to_string();
    let title_for_add = title.to_string();
    div![
        div![
            C!["has-background-dark-transparent"],
            style! {
                St::Cursor => "pointer",
                St::Display => "flex",
                St::AlignItems => "center",
                St::UserSelect => "none",
                St::MarginBottom => "4px",
                St::Width => "100%",
                St::Padding => "2px 8px",
            },
            ev(Ev::Click, move |_| Msg::ToggleSection(toggle_id)),
            i![
                C!["material-icons", "has-text-light"],
                style! { St::FontSize => "28px", St::MarginRight => "8px" },
                icon,
            ],
            span![
                C!["title is-5 has-text-light"],
                style! { St::MarginBottom => "0" },
                title,
            ],
            span![
                C!["has-text-grey-light"],
                style! { St::MarginLeft => "10px", St::FontSize => "0.9rem" },
                format!("({item_count})"),
            ],
            a![
                style! { St::MarginLeft => "auto", St::MarginRight => "12px", St::Display => "flex", St::AlignItems => "center" },
                attrs!(At::Title => "Replace queue & play all"),
                i![
                    C!["material-icons", "has-text-light"],
                    style! { St::FontSize => "24px" },
                    "play_circle_filled",
                ],
                ev(Ev::Click, move |event| {
                    event.stop_propagation();
                    match kind {
                        CategoryKind::Genre => Msg::LoadGenreInQueue(title_for_load),
                        CategoryKind::Decade => Msg::LoadDecadeInQueue(title_for_load),
                    }
                }),
            ],
            a![
                style! { St::MarginRight => "8px", St::Display => "flex", St::AlignItems => "center" },
                attrs!(At::Title => "Add all to queue"),
                i![
                    C!["material-icons", "has-text-light"],
                    style! { St::FontSize => "24px" },
                    "playlist_add",
                ],
                ev(Ev::Click, move |event| {
                    event.stop_propagation();
                    match kind {
                        CategoryKind::Genre => Msg::AddGenreToQueue(title_for_add),
                        CategoryKind::Decade => Msg::AddDecadeToQueue(title_for_add),
                    }
                }),
            ],
        ],
        IF!(is_expanded =>
            if let Some(albums) = albums {
                section![
                    C!["section"],
                    div![
                        C!["carousel"],
                        id!(carousel_id.to_string()),
                        albums.iter().map(view_album_carousel_item)
                    ],
                ]
            } else {
                section![
                    C!["section"],
                    progress![C!["progress", "is-small"], attrs!{ At::Max => "100"}],
                ]
            }
        ),
    ]
}

fn view_album_carousel_item(album: &Album) -> Node<Msg> {
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
        // Headers are not rendered as carousel items
        PlaylistType::GenreHeader(_, _) | PlaylistType::DecadeHeader(_, _) => div![],
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
