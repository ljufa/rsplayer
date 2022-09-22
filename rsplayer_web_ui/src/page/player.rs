use api_models::common::*;
use api_models::player::*;
use api_models::serde::Deserialize;
use api_models::state::*;

use seed::{prelude::*, *};

use std::str::FromStr;

// ------ ------
//     Model
// ------ ------

#[derive(Debug)]
pub struct Model {
    streamer_status: StreamerState,
    player_info: Option<PlayerInfo>,
    current_song: Option<Song>,
    progress: SongProgress,
    waiting_response: bool,
    remote_error: Option<String>,
}
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    StatusChangeEventReceived(StateChangeEvent),
    AlbumImageUpdated(Image),
    SendPlayerCommand(PlayerCommand),
    SendSystemCommand(SystemCommand),
    WebSocketOpen,
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

// ------ ------
//     Init
// ------ ------

pub(crate) fn init(_: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentSong));
    orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentPlayerInfo));
    orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentStreamerState));
    Model {
        streamer_status: StreamerState {
            selected_audio_output: AudioOut::SPKR,
            volume_state: Volume::default(),
        },
        player_info: None,
        current_song: None,
        waiting_response: false,
        remote_error: None,
        progress: Default::default(),
    }
}

// ------ ------
//    Update
// ------ ------

pub(crate) fn update(msg: Msg, mut model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::AlbumImageUpdated(image) => {
            model.current_song.as_mut().unwrap().uri = Some(image.text);
        }

        Msg::StatusChangeEventReceived(StateChangeEvent::CurrentSongEvent(song)) => {
            model.waiting_response = false;
            let ps = song.clone();
            model.current_song = Some(song);
            if ps.uri.is_none() {
                orders.perform_cmd(async { update_album_cover(ps).await });
            }
        }

        Msg::StatusChangeEventReceived(StateChangeEvent::PlayerInfoEvent(player_info)) => {
            model.waiting_response = false;
            model.player_info = Some(player_info);
        }

        Msg::StatusChangeEventReceived(StateChangeEvent::SongTimeEvent(time)) => {
            model.progress = time;
        }

        Msg::StatusChangeEventReceived(StateChangeEvent::StreamerStateEvent(streamer_status)) => {
            model.waiting_response = false;
            model.streamer_status = streamer_status;
        }

        Msg::StatusChangeEventReceived(StateChangeEvent::ErrorEvent(error)) => {
            model.remote_error = Some(error)
        }

        Msg::SendSystemCommand(cmd) => {
            log!("Player {}", cmd);
            if let SystemCommand::SetVol(vol) = cmd {
                model.streamer_status.volume_state.current = vol as i64
            }
            orders.skip();
        }

        Msg::WebSocketOpen => {
            orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentSong));
            orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentPlayerInfo));
            orders.send_msg(Msg::SendPlayerCommand(PlayerCommand::QueryCurrentStreamerState));
        }

        _ => {
            log!("Unknown variant");
            orders.skip();
        }
    }
}

// ------ ------
//     View
// ------ ------
pub(crate) fn view(model: &Model) -> Node<Msg> {
    div![
        style! {
            St::BackgroundImage => get_background_image(model),
            St::BackgroundRepeat => "no-repeat",
            St::BackgroundSize => "cover",
            St::MinHeight => "95vh"
        },
        div![
            style! {
                St::Background => "rgba(86, 92, 86, 0.507)",
                St::MinHeight => "95vh"
            },
            view_track_info(model.current_song.as_ref(), model.player_info.as_ref()),
            view_controls_up(model),
            view_controls_down(model),
        ]
    ]
}

fn view_track_info(status: Option<&Song>, player_info: Option<&PlayerInfo>) -> Node<Msg> {
    if let Some(ps) = status {
        div![
            style! {
                St::MinHeight => "300px",
                St::PaddingTop => "2rem"
            },
            C!["transparent"],
            nav![
                C!["level", "is-flex-direction-column"],
                IF!(ps.title.is_some() =>
                div![
                    C!["level-item has-text-centered"],
                    div![
                        p![
                            C!["is-size-3 has-text-light has-background-dark-transparent"],
                            ps.title.as_ref().map_or("NA", |f| f)
                        ],
                    ],
                ]),
                IF!(ps.album.is_some() =>
                div![
                    C!["level-item"],
                    div![
                        p![
                            C!["has-text-light has-background-dark-transparent"],
                            ps.album.as_ref().map_or("NA", |f| f)
                        ],
                    ],
                ]),
                IF!(ps.artist.is_some() =>
                div![
                    C!["level-item"],
                    div![
                        p![
                            C!["has-text-light has-background-dark-transparent"],
                            ps.artist.as_ref().map_or("NA", |f| f)
                        ],
                    ],
                ]),
                if ps.title.is_none() {
                    div![
                        C!["level-item"],
                        div![p![
                            C!["has-text-light has-background-dark-transparent"],
                            ps.file.clone()
                        ],],
                    ]
                } else {
                    empty!()
                },
            ],
            nav![
                C!["level", "is-flex-direction-column"],
                IF!(ps.genre.is_some() =>
                div![
                    C!["level-item"],
                    div![
                        p![
                            C!["has-text-light has-background-dark-transparent"],
                            ps.genre.as_ref().map_or("NA", |f| f)
                        ],
                    ],
                ]),
                IF!(ps.date.is_some() =>
                div![
                    C!["level-item"],
                    div![
                        p![
                            C!["has-text-light has-background-dark-transparent"],
                            ps.date.as_ref().map_or("NA", |f| f)
                        ],
                    ],
                ]),
                if let Some(pi) = player_info {
                    div![
                        C!["level-item"],
                        IF!(pi.audio_format_rate.is_some() =>
                            div![p![
                            C!["has-text-light has-background-dark-transparent"],
                            format!("Freq: {} | Bit: {} | Ch: {}", pi.audio_format_rate.map_or(0, |f|f),
                            pi.audio_format_bit.map_or(0, |f|f), pi.audio_format_channels.map_or(0,|f|f))
                        ]]),
                    ]
                } else {
                    empty!()
                }
            ],
        ]
    } else {
        empty!()
    }
}

fn view_track_progress_bar(progress: &SongProgress) -> Node<Msg> {
    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        span![
            C![
                "is-size-6",
                "has-text-light",
                "has-background-dark-transparent"
            ],
            progress.format_time()
        ],
        progress![
            C!["progress", "is-small", "is-success"],
            attrs! {"value"=> progress.current_time.as_secs()},
            attrs! {"max"=> progress.total_time.as_secs()},
            progress.current_time.as_secs()
        ],
    ]
}
fn view_controls_down(model: &Model) -> Node<Msg> {
    let playing = model.player_info.as_ref().map_or(false, |f| {
        f.state
            .as_ref()
            .map_or(false, |f| *f == PlayerState::PLAYING)
    });
    div![
        C!["centered", "is-bottom"],
        a![
            C!["player-button-play", "player-button-prev",],
            ev(Ev::Click, |_| Msg::SendPlayerCommand(PlayerCommand::Prev)),
        ],
        a![
            C!["player-button-play", IF!(playing => "player-button-pause" )],
            ev(Ev::Click, move |_| if playing {
                Msg::SendPlayerCommand(PlayerCommand::Pause)
            } else {
                Msg::SendPlayerCommand(PlayerCommand::Play)
            })
        ],
        a![
            C!["player-button-play", "player-button-next"],
            ev(Ev::Click, |_| Msg::SendPlayerCommand(PlayerCommand::Next))
        ],
    ]
}

fn view_controls_up(model: &Model) -> Node<Msg> {
    let audio_out = match &model.streamer_status.selected_audio_output {
        AudioOut::SPKR => "speaker",
        AudioOut::HEAD => "headset",
    };
    let shuffle = model.player_info.as_ref().map_or("shuffle", |r| {
        if r.random.unwrap_or(false) {
            "shuffle"
        } else {
            "format_list_numbered"
        }
    });

    div![
        C!["centered", "box", "has-background-dark-transparent"],
        style! { St::Top => "35%", St::Padding => "10px", St::Width => "80%" },
        view_track_progress_bar(&model.progress),
        view_volume_slider(&model.streamer_status.volume_state),
        div![
            C!["has-text-centered"],
            button![
                C!["small-button"],
                span![C!("icon"), i![C!("fas fa-volume-down")]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolDown))
            ],
            button![
                C!["small-button"],
                span![C!("icon"), i![C!("fas fa-volume-up")]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolUp))
            ],
            button![
                C!["small-button"],
                span![C!["icon"], i![C!("material-icons"), shuffle]],
                ev(Ev::Click, |_| Msg::SendPlayerCommand(PlayerCommand::RandomToggle)),
            ],
            button![
                C!["small-button"],
                span![C!["icon"], i![C!("material-icons"), audio_out]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::ChangeAudioOutput))
            ]
        ]
    ]
}

fn view_volume_slider(volume_state: &Volume) -> Node<Msg> {
    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        span![
            C!["is-size-6", "has-text-light",],
            format!("Volume: {}/{}", volume_state.current, volume_state.max)
        ],
        input![
            C!["slider", "is-fullwidth", "is-success"],
            style! {
                St::PaddingRight => "1.2rem"
            },
            attrs! {"value"=> volume_state.current},
            attrs! {"step"=> volume_state.step},
            attrs! {"max"=> volume_state.max},
            attrs! {"min"=> volume_state.min},
            attrs! {"type"=> "range"},
            input_ev(Ev::Change, move |selected| Msg::SendSystemCommand(
                SystemCommand::SetVol(u8::from_str(selected.as_str()).unwrap())
            )),
        ],
    ]
}

fn get_background_image(model: &Model) -> String {
    if let Some(ps) = model.current_song.as_ref() {
        format!("url({})", ps.uri.as_ref().map_or("/no_album.png", |f| f))
    } else {
        String::new()
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
    let response = fetch(format!("http://ws.audioscrobbler.com/2.0/?method=album.getinfo&album={}&artist={}&api_key=3b3df6c5dd3ad07222adc8dd3ccd8cdc&format=json", album, artist)).await;
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
