use api_models::{
    common::{PlayerCommand, SystemCommand, Volume},
    player::Song,
    state::{AudioOut, PlayerInfo, PlayerState, SongProgress, StateChangeEvent, StreamerState},
};

use seed::{
    a, article, attrs, div, empty, figure, i, img, input, li, log, nav, p, prelude::*, progress,
    span, struct_urls, style, ul, C, IF,
};
use std::str::FromStr;

use serde::Deserialize;
use strum_macros::IntoStaticStr;
extern crate api_models;
mod page;

const SETTINGS: &str = "settings";
const PLAYLIST: &str = "playlist";
const QUEUE: &str = "queue";
const FIRST_SETUP: &str = "setup";
const PLAYER: &str = "player";
// ------ ------
//     Model
// ------ ------
#[derive(Debug)]
pub struct PlayerModel {
    streamer_status: StreamerState,
    player_info: Option<PlayerInfo>,
    current_song: Option<Song>,
    progress: SongProgress,
}

#[derive(Debug)]
struct Model {
    base_url: Url,
    page: Page,
    web_socket: WebSocket,
    web_socket_reconnector: Option<StreamHandle>,
    startup_error: Option<String>,
    player_model: PlayerModel,
}

pub enum Msg {
    WebSocketOpened,
    CloseWebSocket,
    WebSocketMessgeReceived(WebSocketMessage),
    WebSocketClosed(CloseEvent),
    WebSocketFailed,
    ReconnectWebSocket(usize),
    UrlChanged(subs::UrlChanged),
    StatusChangeEventReceived(StateChangeEvent),
    Settings(page::settings::Msg),
    Playlist(page::playlist::Msg),
    StartErrorReceived(String),
    Queue(page::queue::Msg),
    Ignore,

    SendPlayerCommand(PlayerCommand),
    SendSystemCommand(SystemCommand),
    AlbumImageUpdated(Image),
}

#[derive(Debug, Deserialize)]
pub struct AlbumInfo {
    pub album: Album,
}
#[derive(Debug, Deserialize)]
pub struct Album {
    image: Vec<Image>,
}
#[derive(Debug, Deserialize)]
pub struct Image {
    size: String,
    #[serde(rename = "#text")]
    text: String,
}
// ------ Page ------
#[derive(Debug, IntoStaticStr)]
enum Page {
    Home,
    Settings(page::settings::Model),
    Player,
    Playlist(page::playlist::Model),
    Queue(page::queue::Model),
    NotFound,
}

impl Page {
    fn new(url: Url, orders: &mut impl Orders<Msg>) -> Self {
        let slice = url.hash().map_or("", |p| {
            if p.contains('#') {
                p.split_once('#').unwrap().0
            } else {
                p.as_str()
            }
        });
        match slice {
            FIRST_SETUP => Self::Home,
            SETTINGS => Self::Settings(page::settings::init(url, &mut orders.proxy(Msg::Settings))),
            PLAYLIST => Self::Playlist(page::playlist::init(url, &mut orders.proxy(Msg::Playlist))),
            QUEUE => Self::Queue(page::queue::init(url, &mut orders.proxy(Msg::Queue))),
            PLAYER | "" => Self::Player,
            _ => Self::NotFound,
        }
    }

    fn has_image_background(&self) -> bool {
        match self {
            Page::Settings(_) => false,
            _ => true,
        }
    }
}

// ------ ------
//     Init
// ------ ------

fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    let page = Page::new(url.clone(), orders);
    orders
        .subscribe(Msg::UrlChanged)
        .notify(subs::UrlChanged(url.clone()));

    orders.perform_cmd(async {
        let response = fetch("/api/start_error")
            .await
            .expect("failed to get response");
        if response.status().is_ok() {
            Msg::StartErrorReceived(response.text().await.expect(""))
        } else {
            Msg::Ignore
        }
    });

    Model {
        base_url: url.to_base_url(),
        page,
        web_socket: create_websocket(orders),
        web_socket_reconnector: None,
        startup_error: None,
        player_model: PlayerModel {
            streamer_status: StreamerState {
                selected_audio_output: AudioOut::SPKR,
                volume_state: Volume::default(),
            },
            player_info: None,
            current_song: None,
            progress: Default::default(),
        },
    }
}
// ------ ------
//     Urls
// ------ ------

struct_urls!();
impl<'a> Urls<'a> {
    fn settings(self) -> Url {
        self.base_url().add_hash_path_part(SETTINGS)
    }
    fn settings_abs() -> Url {
        Url::new().add_hash_path_part(SETTINGS)
    }
    fn queue_abs() -> Url {
        Url::new().add_hash_path_part(QUEUE)
    }
    fn playlist_abs() -> Url {
        Url::new().add_hash_path_part(PLAYLIST)
    }

    fn player_abs() -> Url {
        Url::new().add_hash_path_part(PLAYER)
    }
}

// ------ ------
//    Update
// ------ ------

fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::WebSocketOpened => {
            model.web_socket_reconnector = None;
            log!("WebSocket connection is open now");
            orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentSong));
            orders.send_msg(Msg::SendPlayerCommand(
                PlayerCommand::QueryCurrentPlayerInfo,
            ));
            orders.send_msg(Msg::SendPlayerCommand(
                PlayerCommand::QueryCurrentStreamerState,
            ));
            if let Page::Queue(model) = &mut model.page {
                page::queue::update(
                    page::queue::Msg::WebSocketOpen,
                    model,
                    &mut orders.proxy(Msg::Queue),
                )
            }
        }

        Msg::CloseWebSocket => {
            model.web_socket_reconnector = None;
            model
                .web_socket
                .close(None, Some("user clicked Close button"))
                .unwrap();
        }

        Msg::WebSocketClosed(close_event) => {
            log!("==================");
            log!("WebSocket connection was closed:");
            log!("Clean:", close_event.was_clean());
            log!("Code:", close_event.code());
            log!("Reason:", close_event.reason());
            log!("==================");

            // Chrome doesn't invoke `on_error` when the connection is lost.
            if !close_event.was_clean() && model.web_socket_reconnector.is_none() {
                model.web_socket_reconnector = Some(
                    orders.stream_with_handle(streams::backoff(None, Msg::ReconnectWebSocket)),
                );
            }
        }

        Msg::WebSocketFailed => {
            log!("WebSocket failed");
            if model.web_socket_reconnector.is_none() {
                model.web_socket_reconnector = Some(
                    orders.stream_with_handle(streams::backoff(None, Msg::ReconnectWebSocket)),
                );
            }
        }

        Msg::ReconnectWebSocket(retries) => {
            log!("Reconnect attempt:", retries);
            model.web_socket = create_websocket(orders);
        }

        Msg::UrlChanged(subs::UrlChanged(url)) => model.page = Page::new(url, orders),

        Msg::AlbumImageUpdated(image) => {
            model.player_model.current_song.as_mut().unwrap().image_url = Some(image.text);
        }

        Msg::StatusChangeEventReceived(chg_ev) => {
            match &chg_ev {
                StateChangeEvent::CurrentSongEvent(song) => {
                    let ps = song.clone();
                    if ps.image_url.is_none() {
                        orders.perform_cmd(async { update_album_cover(ps).await });
                    }
                    model.player_model.current_song = Some(song.clone());
                }
                StateChangeEvent::StreamerStateEvent(sst) => {
                    model.player_model.streamer_status = sst.clone()
                }
                StateChangeEvent::PlayerInfoEvent(pi) => {
                    model.player_model.player_info = Some(pi.clone())
                }
                _ => {}
            }

            if let Page::Queue(model) = &mut model.page {
                page::queue::update(
                    page::queue::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::Queue),
                )
            } else if let Page::Playlist(model) = &mut model.page {
                page::playlist::update(
                    page::playlist::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::Playlist),
                )
            }
        }

        Msg::Settings(msg) => {
            if let Page::Settings(sett_model) = &mut model.page {
                if let page::settings::Msg::SendCommand(cmd) = &msg {
                    _ = model.web_socket.send_json(cmd);
                }
                page::settings::update(msg, sett_model, &mut orders.proxy(Msg::Settings));
            }
        }

        Msg::Playlist(msg) => {
            if let Page::Playlist(player_model) = &mut model.page {
                if let page::playlist::Msg::SendCommand(cmd) = &msg {
                    _ = model.web_socket.send_json(cmd);
                }
                page::playlist::update(msg, player_model, &mut orders.proxy(Msg::Playlist));
            }
        }
        Msg::Queue(msg) => {
            if let Page::Queue(player_model) = &mut model.page {
                if let page::queue::Msg::SendCommand(cmd) = &msg {
                    _ = model.web_socket.send_json(cmd);
                }
                page::queue::update(msg, player_model, &mut orders.proxy(Msg::Queue));
            }
        }

        Msg::WebSocketMessgeReceived(message) => {
            let msg_text = message.text();
            if msg_text.is_ok() {
                let msg = message.json::<StateChangeEvent>().unwrap_or_else(|_| {
                    panic!("Failed to decode WebSocket text message: {msg_text:?}")
                });
                if let StateChangeEvent::SongTimeEvent(st) = msg {
                    model.player_model.progress = st;
                } else {
                    orders.send_msg(Msg::StatusChangeEventReceived(msg));
                }
            }
        }
        Msg::StartErrorReceived(error_msg) => {
            model.startup_error = Some(error_msg);
        }
        Msg::SendPlayerCommand(cmd) => {
            _ = model.web_socket.send_json(&cmd);
        }
        Msg::SendSystemCommand(cmd) => {
            _ = model.web_socket.send_json(&cmd);
            log!("lib {}", cmd);
            if let SystemCommand::SetVol(vol) = cmd {
                model.player_model.streamer_status.volume_state.current = i64::from(vol)
            }
            orders.skip();
        }
        Msg::Ignore => {}
    }
}

// ------ ------
//     View
// ------ ------
fn view(model: &Model) -> impl IntoNodes<Msg> {
    div![
        IF!(
            model.page.has_image_background() =>
            style! {
                St::BackgroundImage => get_background_image(&model.player_model),
                St::BackgroundRepeat => "no-repeat",
                St::BackgroundSize => "cover",
                St::MinHeight => "95vh"
            }
        ),
        C!["container"],
        view_navigation_tabs(&model.page),
        view_startup_error(model.startup_error.as_ref()),
        view_content(model, &model.base_url),
        view_player_footer(&model.page, &model.player_model)
    ]
}

fn view_player_footer(page: &Page, player_model: &PlayerModel) -> Node<Msg> {
    if let Page::Player = page {
        return empty!();
    };
    let playing = player_model.player_info.as_ref().map_or(false, |f| {
        f.state
            .as_ref()
            .map_or(false, |f| *f == PlayerState::PLAYING)
    });
    let shuffle_class = player_model.player_info.as_ref().map_or("fa-shuffle", |r| {
        if r.random.unwrap_or(false) {
            "fa-shuffle"
        } else {
            "fa-list-ol"
        }
    });

    div![
        C!["page-foot", "container"],
        nav![
            style! {
                St::Width => "100%"
            },
            C!["level", "is-mobile"],
            // image
            div![
                C!["level-left", "is-flex-grow-1", "is-hidden-mobile"],
                div![
                    C!["level-item"],
                    figure![
                        C!["image", "is-64x64"],
                        img![attrs! {"src" => get_album_image(player_model)}]
                    ]
                ],
            ],
            // track info
            div![
                C!["level-left", "is-flex-grow-3"],
                div![
                    C!["level-item", "available-width"],
                    div![
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model
                                .current_song
                                .as_ref()
                                .map_or("".to_string(), |f| f.get_title())
                        ],
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model
                                .current_song
                                .as_ref()
                                .map_or("".to_string(), |fa| fa
                                    .album
                                    .as_ref()
                                    .map_or("".to_string(), |a| a.clone()))
                        ],
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model
                                .current_song
                                .as_ref()
                                .map_or("".to_string(), |fa| fa
                                    .artist
                                    .as_ref()
                                    .map_or("".to_string(), |a| a.clone()))
                        ],
                    ]
                ],
            ],
            // track progress
            div![
                C!["level-left", "is-flex-grow-5", "is-hidden-mobile"],
                div![
                    C!["level-item"],
                    div![
                        C!["has-text-centered", "available-width"],
                        span![
                            C![
                                "is-size-6",
                                "has-text-light",
                                "has-background-dark-transparent"
                            ],
                            player_model.progress.format_time()
                        ],
                        progress![
                            C!["progress", "is-small", "is-success"],
                            attrs! {"value"=> player_model.progress.current_time.as_secs()},
                            attrs! {"max"=> player_model.progress.total_time.as_secs()},
                            player_model.progress.current_time.as_secs()
                        ],
                    ]
                ]
            ],
            // player controls
            div![
                C!["level-right", "is-flex-grow-3"],
                div![
                    C!["level-item"],
                    div![
                        div![
                            i![
                                C!["fas", "is-clickable", "small-button-footer", shuffle_class],
                                ev(Ev::Click, |_| Msg::SendPlayerCommand(
                                    PlayerCommand::RandomToggle
                                )),
                            ],
                            i![
                                C!["fas", "is-clickable", "fa-backward", "small-button-footer"],
                                ev(Ev::Click, |_| Msg::SendPlayerCommand(PlayerCommand::Prev)),
                            ],
                            i![
                                C![
                                    "fa",
                                    "is-clickable",
                                    "small-button-footer",
                                    IF!(playing => "fa-pause" ),
                                    IF!(!playing => "fa-play" )
                                ],
                                ev(Ev::Click, move |_| if playing {
                                    Msg::SendPlayerCommand(PlayerCommand::Pause)
                                } else {
                                    Msg::SendPlayerCommand(PlayerCommand::Play)
                                })
                            ],
                            i![
                                C!["fas", "is-clickable", "fa-forward", "small-button-footer"],
                                ev(Ev::Click, |_| Msg::SendPlayerCommand(PlayerCommand::Next)),
                            ],
                        ],
                        div![input![
                            C!["slider", "is-success"],
                            attrs! {"value"=> player_model.streamer_status.volume_state.current},
                            attrs! {"step"=> player_model.streamer_status.volume_state.step},
                            attrs! {"max"=> player_model.streamer_status.volume_state.max},
                            attrs! {"min"=> player_model.streamer_status.volume_state.min},
                            attrs! {"type"=> "range"},
                            input_ev(Ev::Change, move |selected| Msg::SendSystemCommand(
                                SystemCommand::SetVol(
                                    u8::from_str(selected.as_str()).unwrap_or_default()
                                )
                            )),
                        ],],
                    ]
                ],
            ],
            div![
                C!["level-right", "is-flex-grow-1"],
                div![
                    C!["level-item"],
                    i![
                        C![
                            "fas",
                            "is-clickable",
                            "fa-up-right-and-down-left-from-center"
                        ],
                        ev(Ev::Click, |_| { Urls::player_abs().go_and_load() })
                    ],
                ]
            ]
        ]
    ]
}

fn view_startup_error(error_msg: Option<&String>) -> Node<Msg> {
    error_msg.map_or(empty!(), |error| {
        article![
            C!["message", "is-danger"],
            div![
                C!["message-header"],
                p![
                    "Startup error! Please check  ",
                    a![
                        "settings.",
                        ev(Ev::Click, |_| { Urls::settings_abs().go_and_load() }),
                    ]
                ],
            ],
            div![C!["message-body"], error]
        ]
    })
}
// ----- view_content ------

fn view_content(main_model: &Model, base_url: &Url) -> Node<Msg> {
    let page = &main_model.page;
    div![
        style! {
            St::Background => "rgba(86, 92, 86, 0.507)",
            St::MinHeight => "95vh"
        },
        C!["main-content"],
        match page {
            Page::Home => page::home::view(base_url),
            Page::NotFound => page::not_found::view(),
            Page::Settings(model) => page::settings::view(model).map_msg(Msg::Settings),
            Page::Player => page::player::view(&main_model.player_model),
            Page::Playlist(model) => page::playlist::view(model).map_msg(Msg::Playlist),
            Page::Queue(model) => page::queue::view(model).map_msg(Msg::Queue),
        }
    ]
}

fn view_navigation_tabs(page: &Page) -> Node<Msg> {
    let page_name: &str = page.into();
    div![
        style! {
            St::Background => "rgba(86, 92, 86, 0.507)",
        },
        C!["tabs", "is-toggle", "is-centered", "is-fullwidth"],
        ul![
            li![
                IF!(page_name == "Player" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![
                        C!["material-icons"],
                        attrs!("aria-hidden" => "true"),
                        "music_note"
                    ],
                ]],
                ev(Ev::Click, |_| { Urls::player_abs().go_and_load() }),
            ],
            li![
                IF!(page_name == "Queue" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![
                        C!["material-icons"],
                        attrs!("aria-hidden" => "true"),
                        "queue_music"
                    ],
                ]],
                ev(Ev::Click, |_| { Urls::queue_abs().go_and_load() }),
            ],
            li![
                IF!(page_name == "Playlist" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![
                        C!["material-icons"],
                        attrs!("aria-hidden" => "true"),
                        "library_music"
                    ],
                ],],
                ev(Ev::Click, |_| { Urls::playlist_abs().go_and_load() }),
            ],
            li![
                IF!(page_name == "Settings" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![
                        C!["material-icons"],
                        attrs!("aria-hidden" => "true"),
                        "tune"
                    ],
                ]],
                ev(Ev::Click, |_| { Urls::settings_abs().go_and_load() }),
            ],
        ]
    ]
}
pub fn view_spinner_modal<Ms>(active: bool) -> Node<Ms> {
    // spinner
    div![
        C!["modal", IF!(active => "is-active")],
        div![C!["modal-background"]],
        div![
            C!["modal-content"],
            div![
                C!("sk-fading-circle"),
                div![C!["sk-circle1 sk-circle"]],
                div![C!["sk-circle2 sk-circle"]],
                div![C!["sk-circle3 sk-circle"]],
                div![C!["sk-circle4 sk-circle"]],
                div![C!["sk-circle5 sk-circle"]],
                div![C!["sk-circle6 sk-circle"]],
                div![C!["sk-circle7 sk-circle"]],
                div![C!["sk-circle8 sk-circle"]],
                div![C!["sk-circle9 sk-circle"]],
                div![C!["sk-circle10 sk-circle"]],
                div![C!["sk-circle11 sk-circle"]],
                div![C!["sk-circle12 sk-circle"]],
            ]
        ]
    ]
}

// ------ ------
//     Start
// ------ ------

#[wasm_bindgen(start)]
pub fn start() {
    App::start("app", init, update, view);
}

// ------ ------
//    Extern
// ------ ------

#[wasm_bindgen]
extern "C" {
    pub fn scrollToId(id: &str);
    pub fn attachCarousel(id: &str);
}

fn create_websocket(orders: &impl Orders<Msg>) -> WebSocket {
    if let Ok(current) = seed::browser::util::window().location().href() {
        let start = current.find("//").unwrap_or(0) + 2;
        let end = current.find("/#").unwrap_or(current.len());
        let url = &current[start..end];
        let mut ws_url = String::from("ws://");
        ws_url.push_str(url);
        if url.ends_with('/') {
            ws_url.push_str("api/ws");
        } else {
            ws_url.push_str("/api/ws");
        }
        WebSocket::builder(ws_url, orders)
            .on_open(|| Msg::WebSocketOpened)
            .on_message(Msg::WebSocketMessgeReceived)
            .on_close(Msg::WebSocketClosed)
            .on_error(|| Msg::WebSocketFailed)
            .build_and_open()
            .unwrap()
    } else {
        panic!("No url found");
    }
}

async fn update_album_cover(track: Song) -> Msg {
    if track.album.is_some() && track.artist.is_some() {
        let ai = get_album_image_from_lastfm_api(track.album.unwrap(), track.artist.unwrap()).await;
        match ai {
            Some(ai) => Msg::AlbumImageUpdated(ai),
            None => Msg::AlbumImageUpdated(Image {
                size: "mega".to_string(),
                text: "/no_album.png".to_string(),
            }),
        }
    } else {
        Msg::AlbumImageUpdated(Image {
            size: "mega".to_string(),
            text: "/no_album.png".to_string(),
        })
    }
}

async fn get_album_image_from_lastfm_api(album: String, artist: String) -> Option<Image> {
    let response = fetch(format!("http://ws.audioscrobbler.com/2.0/?method=album.getinfo&album={album}&artist={artist}&api_key=3b3df6c5dd3ad07222adc8dd3ccd8cdc&format=json")).await;
    if let Ok(response) = response {
        let info = response.json::<AlbumInfo>().await;
        if let Ok(info) = info {
            info.album
                .image
                .into_iter()
                .find(|i| i.size == "mega" && !i.text.is_empty())
        } else {
            log!("Failed to get album info {}", info);
            None
        }
    } else if let Err(e) = response {
        log!("Error getting album info from last.fm {}", e);
        None
    } else {
        None
    }
}
fn get_background_image(model: &PlayerModel) -> String {
    if let Some(ps) = model.current_song.as_ref() {
        format!(
            "url({})",
            ps.image_url.as_ref().map_or("/no_album.png", |f| f)
        )
    } else {
        String::new()
    }
}

fn get_album_image(model: &PlayerModel) -> String {
    if let Some(ps) = model.current_song.as_ref() {
        ps.image_url
            .as_ref()
            .map_or("/no_album.png", |f| f)
            .to_string()
    } else {
        String::new()
    }
}
