use api_models::common::PlayerCommand;
use api_models::common::QueueCommand::RemoveItem;
use api_models::common::UserCommand;
use api_models::player::Song;
use api_models::playlist::PlaylistPage;
use api_models::state::{CurrentQueueQuery, StateChangeEvent};
use gloo_console::log;
use gloo_net::Error;
use seed::prelude::web_sys::KeyboardEvent;
use seed::{
    a, attrs, button, div, empty, footer, header, i, id, input, p, prelude::*, progress, section, span, style,
    textarea, C, IF,
};

use crate::scrollToId;

#[derive(Debug)]
pub struct Model {
    current_queue: Option<PlaylistPage>,
    current_song_id: Option<String>,
    search_input: String,
    waiting_response: bool,
    show_add_url_modal: bool,
    add_url_input: String,
    show_save_playlist_modal: bool,
    save_playlist_input: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    CurrentQueueFetched(Result<Option<PlaylistPage>, Error>),
    SendUserCommand(UserCommand),
    PlaylistItemSelected(String),
    PlaylistItemRemove(String),
    PlaylistItemShowMore,
    StatusChangeEventReceived(StateChangeEvent),
    WebSocketOpen,
    SearchInputChanged(String),
    DoSearch,
    ClearSearch,
    ShowStartingFromCurrentSong,
    LocateCurrentSong,
    LoadMoreItems(usize),
    AddUrlButtonClick,
    AddUrlInputChanged(String),
    AddUrlToQueue,
    CloseAddUrlModal,
    ClearQueue,
    SaveAsPlaylistButtonClick,
    SaveAsPlaylistInputChanged(String),
    CloseSaveAsPlaylistModal,
    SaveAsPlaylist,
    KeyPressed(web_sys::KeyboardEvent),
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    log!("Queue: init");
    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
        api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(String::default(), 0)),
    )));
    orders.stream(streams::window_event(Ev::KeyDown, |event| {
        Msg::KeyPressed(event.unchecked_into())
    }));

    Model {
        current_queue: None,
        current_song_id: None,
        waiting_response: true,
        search_input: String::default(),
        show_add_url_modal: false,
        add_url_input: String::default(),
        show_save_playlist_modal: false,
        save_playlist_input: String::default(),
    }
}

// ------ ------
//    Update
// ------ ------

#[allow(clippy::too_many_lines)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::CurrentQueueEvent(pc)) => {
            model.waiting_response = false;
            model.current_queue = pc;
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::CurrentSongEvent(evt)) => {
            model.waiting_response = false;
            model.current_song_id = Some(evt.file);
            // orders.after_next_render(|_| scrollToId("current"));
        }
        Msg::PlaylistItemSelected(id) => {
            model.current_song_id = Some(id.clone());
            orders.send_msg(Msg::SendUserCommand(UserCommand::Player(PlayerCommand::PlayItem(id))));
        }
        Msg::PlaylistItemRemove(id) => {
            if let Some(queue) = model.current_queue.as_mut() {
                queue.remove_item(&id);
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(RemoveItem(id))));
            }
        }
        Msg::WebSocketOpen => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    String::default(),
                    0,
                )),
            )));
            // orders.after_next_render(|_| scrollToId("current"));
            orders.skip();
        }
        Msg::SearchInputChanged(term) => {
            model.search_input = term;
            orders.skip();
        }
        Msg::DoSearch => {
            model.waiting_response = true;
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    model.search_input.clone(),
                    0,
                )),
            )));
        }
        Msg::ShowStartingFromCurrentSong => {
            model.waiting_response = true;
            model.search_input = String::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::CurrentSongPage),
            )));
        }
        Msg::ClearSearch => {
            model.waiting_response = true;
            model.search_input = String::new();
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    String::new(),
                    0,
                )),
            )));
        }
        Msg::LocateCurrentSong => {
            scrollToId("current");
        }
        Msg::LoadMoreItems(offset) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    model.search_input.clone(),
                    offset,
                )),
            )));
            orders.after_next_render(move |_| scrollToId("top-list-item"));
        }
        Msg::AddUrlButtonClick => {
            model.show_add_url_modal = true;
            model.add_url_input.clear();
        }
        Msg::CloseAddUrlModal => {
            model.show_add_url_modal = false;
        }
        Msg::KeyPressed(event) => {
            if event.key() == "Escape" {
                model.show_add_url_modal = false;
                model.add_url_input = String::default();
                model.show_save_playlist_modal = false;
                model.save_playlist_input = String::default();
            }
        }
        Msg::AddUrlInputChanged(value) => {
            model.add_url_input = value;
        }
        Msg::AddUrlToQueue => {
            if model.show_add_url_modal && model.add_url_input.len() > 3 {
                if model.add_url_input.lines().count() > 1 {
                    model.add_url_input.lines().for_each(|l| {
                        if l.len() > 5 {
                            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                                api_models::common::QueueCommand::AddSongToQueue(l.to_string()),
                            )));
                        }
                    });
                } else {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                        api_models::common::QueueCommand::AddSongToQueue(model.add_url_input.clone()),
                    )));
                }
                model.show_add_url_modal = false;
            }
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    String::default(),
                    0,
                )),
            )));
        }
        Msg::SaveAsPlaylistButtonClick => {
            model.show_save_playlist_modal = true;
        }
        Msg::SaveAsPlaylistInputChanged(value) => {
            model.save_playlist_input = value;
        }
        Msg::CloseSaveAsPlaylistModal => {
            model.save_playlist_input = String::default();
            model.show_save_playlist_modal = false;
        }
        Msg::SaveAsPlaylist => {
            if model.show_save_playlist_modal && model.save_playlist_input.len() > 3 {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Playlist(
                    api_models::common::PlaylistCommand::SaveQueueAsPlaylist(model.save_playlist_input.clone()),
                )));
                model.show_save_playlist_modal = false;
                model.save_playlist_input = String::default();
            }
        }
        Msg::ClearQueue => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::ClearQueue,
            )));
            orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(
                api_models::common::QueueCommand::QueryCurrentQueue(CurrentQueueQuery::WithSearchTerm(
                    String::default(),
                    0,
                )),
            )));
        }

        _ => {
            orders.skip();
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![
        view_add_url_modal(model),
        view_save_playlist_modal(model),
        view_queue_items(model)
    ]
}

fn view_save_playlist_modal(model: &Model) -> Node<Msg> {
    div![
        C!["modal", IF!(model.show_save_playlist_modal => "is-active")],
        div![C!["modal-background"], ev(Ev::Click, |_| Msg::CloseSaveAsPlaylistModal),],
        div![
            id!("add-url-items-modal"),
            C!["modal-card"],
            header![
                C!["modal-card-head"],
                p![C!["modal-card-title"], "Enter playlist name"],
                button![
                    C!["delete", "is-large"],
                    attrs!(At::AriaLabel =>"close"),
                    ev(Ev::Click, |_| Msg::CloseSaveAsPlaylistModal)
                ],
            ],
            section![
                C!["modal-card-body"],
                input![
                    C!["input"],
                    attrs! {
                        At::AutoFocus => true.as_at_value();
                    },
                    input_ev(Ev::Input, Msg::SaveAsPlaylistInputChanged),
                    ev(Ev::KeyDown, |keyboard_event| {
                        if keyboard_event.value_of().to_string() == "[object KeyboardEvent]" {
                            let kev: KeyboardEvent = keyboard_event.unchecked_into();
                            IF!(kev.key_code() == 13 => Msg::SaveAsPlaylist)
                        } else {
                            None
                        }
                    }),
                ],
            ],
            footer![
                C!["modal-card-foot"],
                button![
                    C!["button", "is-dark"],
                    "Save",
                    ev(Ev::Click, move |_| Msg::SaveAsPlaylist)
                ],
                button![
                    C!["button"],
                    "Cancel",
                    ev(Ev::Click, move |_| Msg::CloseSaveAsPlaylistModal)
                ],
            ]
        ]
    ]
}

fn view_add_url_modal(model: &Model) -> Node<Msg> {
    if model.show_add_url_modal {
        div![
            C!["modal", "is-active"],
            div![C!["modal-background"], ev(Ev::Click, |_| Msg::CloseAddUrlModal),],
            div![
                id!("add-url-items-modal"),
                C!["modal-card"],
                header![
                    C!["modal-card-head"],
                    p![C!["modal-card-title"], "Add streaming URL(s)"],
                    button![
                        C!["delete", "is-medium"],
                        attrs!(At::AriaLabel =>"close"),
                        ev(Ev::Click, |_| Msg::CloseAddUrlModal)
                    ],
                ],
                section![
                    C!["modal-card-body"],
                    textarea![
                        C!["textarea"],
                        attrs! {
                            At::AutoFocus => true.as_at_value();
                        },
                        input_ev(Ev::Input, Msg::AddUrlInputChanged),
                        ev(Ev::KeyDown, |keyboard_event| {
                            if keyboard_event.value_of().to_string() == "[object KeyboardEvent]" {
                                let kev: KeyboardEvent = keyboard_event.unchecked_into();
                                IF!(kev.key_code() == 13 => Msg::AddUrlToQueue)
                            } else {
                                None
                            }
                        }),
                    ],
                ],
                footer![
                    C!["modal-card-foot"],
                    button![
                        C!["button", "is-dark"],
                        "Add",
                        ev(Ev::Click, move |_| Msg::AddUrlToQueue)
                    ],
                    button![C!["button"], "Cancel", ev(Ev::Click, move |_| Msg::CloseAddUrlModal)],
                ]
            ]
        ]
    } else {
        empty!()
    }
}

#[allow(clippy::too_many_lines)]
fn view_queue_items(model: &Model) -> Node<Msg> {
    if model.current_queue.is_none() {
        return empty!();
    }
    div![
        div![
            IF!(model.waiting_response => progress![C!["progress", "is-small"], attrs!{ At::Max => "100"}, style!{ St::MarginBottom => "50px"}]),
        ],
        div![
            model.current_queue.as_ref().map_or_else(|| empty!(), |page| {
                let offset = page.offset;
                let iter = page.items.iter();
                div![
                    div![
                        C!["transparent is-flex is-justify-content-center has-background-dark-transparent"],
                        div![C!["control"],
                            input![
                                C!["input", "input-size"],
                                attrs! {
                                    At::Value => model.search_input,
                                    At::Name => "search",
                                    At::Type => "text",
                                    At::Placeholder => "Find a song"
                                },
                                input_ev(Ev::Input, Msg::SearchInputChanged),
                                ev(Ev::KeyDown, |keyboard_event| {
                                    if keyboard_event.value_of().to_string() == "[object KeyboardEvent]"{
                                        let kev: KeyboardEvent = keyboard_event.unchecked_into();
                                        IF!(kev.key_code() == 13 => Msg::DoSearch)
                                    } else {
                                        None
                                    }
                                }),
                            ],
                        ],
                        div![C!["control"],
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
                    ],
                    div![
                        C!["transparent field is-flex is-justify-content-center has-background-dark-transparent"],
                        div![C!["control"],
                            a![
                                attrs!(At::Title => "Add URL to queue"),
                                i![C!["pr-3","pl-2","material-icons","is-large-icon", "white-icon"], "queue"],
                                ev(Ev::Click, move |_| Msg::AddUrlButtonClick)
                            ],
                            a![
                                attrs!(At::Title =>"Save queue as playlist"),
                                i![C!["pr-3","material-icons","is-large-icon", "white-icon"], "save"],
                                ev(Ev::Click, move |_| Msg::SaveAsPlaylistButtonClick)
                            ],
                            a![
                                attrs!(At::Title =>"Show queue starting from current song"),
                                i![C!["pr-3","material-icons","is-large-icon", "white-icon"], "filter_center_focus"],
                                ev(Ev::Click, move |_| Msg::ShowStartingFromCurrentSong)
                            ],
                            a![
                                attrs!(At::Title =>"Clear queue"),
                                i![C!["pr-3", "material-icons","is-large-icon", "white-icon"], "clear"],
                                ev(Ev::Click, move |_| Msg::ClearQueue)
                            ],
                        ]
                    ],

                    // queue items`
                    div![C!["scroll-list list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
                        div![id!("top-list-item")],
                        iter.map(|it| { view_queue_item(it, model)  })
                    ],
                    button![
                        C!["button","is-fullwidth", "is-outlined", "is-primary"],
                        "Load more", 
                        ev(Ev::Click, move |_| Msg::LoadMoreItems(offset))
                    ]
                ]
            })
        ]
    ]
}

fn view_queue_item(song: &Song, model: &Model) -> Node<Msg> {
    let id = song.file.clone();
    let id1 = song.file.clone();
    let id2 = song.file.clone();
    div![
        IF!(model.current_song_id.as_ref().is_some_and(|cur| *cur == id ) => id!("current")),
        C![
            "list-item",
            IF!(model.current_song_id.as_ref().is_some_and(|cur| *cur == id ) => "current")
        ],
        div![
            C!["list-item-content"],
            div![
                C!["list-item-title", "has-text-light"],
                span![&song.get_title()],
                &song.date.as_ref().map(|d| span![format!(" ({d})")]),
                &song
                    .time
                    .as_ref()
                    .map(|t| span![format!(" [{}]", api_models::common::dur_to_string(t))]),
            ],
            ev(Ev::Click, move |_| Msg::PlaylistItemSelected(id)),
        ],
        div![
            C!["list-item-controls"],
            div![
                a![
                    C!["white-icon"],
                    C!["is-hidden-mobile"],
                    attrs!(At::Title =>"Play song"),
                    i![C!("material-icons"), "play_arrow"],
                    ev(Ev::Click, move |_| Msg::PlaylistItemSelected(id2))
                ],
                a![
                    C!["white-icon"],
                    attrs!(At::Title =>"Remove song from queue"),
                    i![C!("material-icons"), "delete"],
                    ev(Ev::Click, move |_| Msg::PlaylistItemRemove(id1))
                ],
            ]
        ],
    ]
}
