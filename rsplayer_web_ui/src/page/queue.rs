use api_models::player::Song;
use api_models::state::{PlayingContext, PlayingContextQuery, StateChangeEvent};
use api_models::{common::Command, state::PlayingContextType};
use seed::prelude::web_sys::KeyboardEvent;
use seed::{prelude::*, *};

use crate::scrollToId;

#[derive(Debug)]
pub struct Model {
    playing_context: Option<PlayingContext>,
    current_song_id: Option<String>,
    search_input: String,
    waiting_response: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    PlayingContextFetched(fetch::Result<Option<PlayingContext>>),
    SendCommand(Command),
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
}

pub(crate) fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    log!("Queue: init");
    orders.send_msg(Msg::SendCommand(Command::QueryCurrentSong));
    orders.send_msg(Msg::SendCommand(Command::QueryCurrentPlayingContext(
        PlayingContextQuery::WithSearchTerm(Default::default(), 0),
    )));
    Model {
        playing_context: None,
        current_song_id: None,
        waiting_response: true,
        search_input: Default::default(),
    }
}

// ------ ------
//    Update
// ------ ------

pub(crate) fn update(msg: Msg, mut model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::CurrentPlayingContextEvent(pc)) => {
            model.waiting_response = false;
            model.playing_context = Some(pc);
            // orders.after_next_render(|_| scrollToId("current"));
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::CurrentSongEvent(evt)) => {
            model.waiting_response = false;
            model.current_song_id = Some(evt.id);
            // orders.after_next_render(|_| scrollToId("current"));
        }
        Msg::PlaylistItemSelected(id) => {
            model.current_song_id = Some(id.clone());
            orders.send_msg(Msg::SendCommand(Command::PlayItem(id)));
        }
        Msg::PlaylistItemRemove(id) => {
            model.playing_context.as_mut().map(|ctx| {
                ctx.playlist_page.as_mut().map(|page| {
                    page.remove_item(id.clone());
                })
            });
            orders.send_msg(Msg::SendCommand(Command::RemovePlaylistItem(id)));
        }
        Msg::WebSocketOpen => {
            orders.send_msg(Msg::SendCommand(Command::QueryCurrentSong));
            orders.send_msg(Msg::SendCommand(Command::QueryCurrentPlayingContext(
                PlayingContextQuery::WithSearchTerm(Default::default(), 0),
            )));
            // orders.after_next_render(|_| scrollToId("current"));
            orders.skip();
        }
        Msg::SearchInputChanged(term) => {
            model.search_input = term;
            orders.skip();
            log!("UpdateInputChanged", model.search_input);
        }
        Msg::DoSearch => {
            model.waiting_response = true;
            orders.send_msg(Msg::SendCommand(Command::QueryCurrentPlayingContext(
                PlayingContextQuery::WithSearchTerm(model.search_input.clone(), 0),
            )));
        }
        Msg::ShowStartingFromCurrentSong => {
            model.waiting_response = true;
            model.search_input = "".to_string();
            orders.send_msg(Msg::SendCommand(Command::QueryCurrentPlayingContext(
                PlayingContextQuery::CurrentSongPage,
            )));
        }
        Msg::ClearSearch => {
            model.waiting_response = true;
            model.search_input = "".to_string();
            orders.send_msg(Msg::SendCommand(Command::QueryCurrentPlayingContext(
                PlayingContextQuery::WithSearchTerm("".to_string(), 0),
            )));
        }
        Msg::LocateCurrentSong => {
            scrollToId("current");
        }

        _ => {
            orders.skip();
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    log!("Queue: view");
    div![
        crate::view_spinner_modal(model.waiting_response),
        view_queue_items(model)
    ]
}

fn view_context_info(
    context_type: &PlayingContextType,
    playing_context: &PlayingContext,) -> Vec<Node<Msg>> {
    match context_type {
        PlayingContextType::Playlist {
            description,
            public,
            ..
        } => nodes![
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Playlist: "),
                b!(&playing_context.name)
            ],
            description.as_ref().map_or(empty!(), |d| p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Description: "),
                b!(d)
            ]),
            public.map_or(empty!(), |p| p![
                C!["has-text-light has-background-dark-transparent"],
                IF!(p => i!("Public playlist")),
            ]),
        ],

        PlayingContextType::Album {
            artists,
            release_date,
            label,
            genres,
        } => nodes![
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Artists: "),
                b!(artists.join(", "))
            ],
            if !genres.is_empty() {
                p![
                    C!["has-text-light has-background-dark-transparent"],
                    i!("Genres: "),
                    b!(genres.join(", "))
                ]
            } else {
                empty!()
            },
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Album: "),
                b!(&playing_context.name.clone())
            ],
            label.as_ref().map_or(empty!(), |l| p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Label: "),
                b!(l)
            ]),
            if !release_date.is_empty() {
                p![
                    C!["has-text-light has-background-dark-transparent"],
                    i!("Release date: "),
                    b!(release_date)
                ]
            } else {
                empty!()
            },
        ],

        PlayingContextType::Artist {
            genres,
            popularity,
            followers,
            description,
        } => nodes![
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Artist: "),
                b!(playing_context.name.clone())
            ],
            if !genres.is_empty() {
                p![
                    C!["has-text-light has-background-dark-transparent"],
                    i!("Genres: "),
                    b!(genres.join(", "))
                ]
            } else {
                empty!()
            },
            description.as_ref().map_or(empty!(), |d| p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Description: "),
                b!(d)
            ]),
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Popularity: "),
                b!(popularity)
            ],
            p![
                C!["has-text-light has-background-dark-transparent"],
                i!("Number of followers: "),
                b!(followers)
            ],
        ],
        _ => return nodes![],
    }
}

fn view_queue_items(model: &Model) -> Node<Msg> {
    if model.playing_context.is_none() {
        return empty!();
    }
    let pctx = model.playing_context.as_ref().unwrap();
    div![
        IF!(pctx.image_url.is_some() =>
            style! {
                St::BackgroundImage => format!("url({})",pctx.image_url.as_ref().unwrap()),
                St::BackgroundRepeat => "no-repeat",
                St::BackgroundSize => "cover",
                St::MinHeight => "95vh"
            }
        ),
        div![
            style! {
                St::Background => "rgba(86, 92, 86, 0.507)",
                St::MinHeight => "95vh"
            },
            div![section![
                C!["transparent"],
                div![span![
                    C!["has-text-light has-background-dark-transparent"],
                    i!["Player: "],
                    b![&pctx.player_type.to_string()]
                ]],
                view_context_info(&pctx.context_type, pctx),
            ],],
            if pctx.playlist_page.is_some() {
                let iter = pctx.playlist_page.as_ref().unwrap().items.iter();
                div![
                    div![
                        C!["transparent field has-addons"],
                        div![C!["control"],
                            input![
                                C!["input"],
                                attrs! {
                                    At::Value => model.search_input,
                                    At::Name => "search",
                                    At::Type => "text",
                                    At::Placeholder => "Find a song"
                                },
                                input_ev(Ev::Input, move |val| Msg::SearchInputChanged(val)),
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
                                i![C!("material-icons is-large-icon"), "search"],
                                ev(Ev::Click, move |_| Msg::DoSearch)
                            ],
                            a![
                                attrs!(At::Title =>"Clear search / Show all songs"),
                                i![C!("material-icons is-large-icon"), "backspace"],
                                ev(Ev::Click, move |_| Msg::ClearSearch)
                            ],
                            a![
                                attrs!(At::Title =>"Show queue starting from current song"),
                                i![C!("material-icons is-large-icon"), "filter_center_focus"],
                                ev(Ev::Click, move |_| Msg::ShowStartingFromCurrentSong)
                            ], 
                            a![
                                attrs!(At::Title =>"Locate current song"),
                                i![C!("material-icons is-large-icon"), "adjust"],
                                ev(Ev::Click, move |_| Msg::LocateCurrentSong)
                            ],
                        ],
                    ],
                    div![C!["scroll-list list has-overflow-ellipsis has-visible-pointer-controls has-hoverable-list-items"],
                        iter.map(|it| { view_queue_item(it, pctx, model)  })
                    ]
                ]
            } else {
                empty!()
            }
        ]
    ]
}

fn view_queue_item(song: &Song, playing_context: &PlayingContext, model: &Model) -> Node<Msg> {
    let id = song.id.clone();
    let id1 = song.id.clone();
    let id2 = song.id.clone();
    div![
        IF!(model.current_song_id.as_ref().map_or(false,|cur| *cur == id ) => id!("current")),
        C![
            "list-item",
            IF!(model.current_song_id.as_ref().map_or(false,|cur| *cur == id ) => "current")
        ],
        div![
            C!["list-item-content", "has-background-dark-transparent"],
            div![
                C!["list-item-title", "has-text-light"],
                span![&song.get_title()],
                &song.date.as_ref().map(|d| span![format!(" ({d})")]),
                &song
                    .time
                    .as_ref()
                    .map(|t| span![format!(" [{}]", api_models::common::dur_to_string(t))]),
            ],
            div![
                C!["description", "has-text-light"],
                match playing_context.context_type {
                    PlayingContextType::Playlist { .. } => span![
                        song.artist.as_ref().map(|at| span![i!["Art: "], at]),
                        song.album.as_ref().map(|a| span![i![" | Alb: "], a]),
                    ],
                    PlayingContextType::Artist { .. } =>
                        span![song.album.as_ref().map(|a| span![i!["Album: "], a]),],
                    _ => empty!(),
                },
            ],
            ev(Ev::Click, move |_| Msg::PlaylistItemSelected(id)),
        ],
        div![
            C!["list-item-controls"],
            div![
                C!["buttons"],
                a![C!["is-hidden-mobile"],
                    attrs!(At::Title =>"Play song"),
                    C!["icon"], i![C!("material-icons"), "play_arrow"],
                    ev(Ev::Click, move |_| Msg::PlaylistItemSelected(id2))
                ],
                a![
                    attrs!(At::Title =>"Remove song from queue"),
                    C!["icon"], i![C!("material-icons"), "delete"],
                    ev(Ev::Click, move |_| Msg::PlaylistItemRemove(id1))
                ],
            ]
        ],
    ]
}
