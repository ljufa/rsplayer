use api_models::common::{PlayerCommand, SystemCommand, Volume};
use api_models::player::Song;

use api_models::state::{AudioOut, PlayerInfo, PlayerState, SongProgress};

use seed::{
    a, attrs, button, div, empty, i, input, nav, p, prelude::*, progress, span, style, C, IF,
};

use std::str::FromStr;

use crate::{Msg, PlayerModel};

// ------ ------
//     View
// ------ ------
pub fn view(model: &PlayerModel) -> Node<Msg> {
    div![div![
        view_track_info(model.current_song.as_ref(), model.player_info.as_ref()),
        view_controls_up(model),
        view_controls_down(model),
    ]]
}

fn view_track_info(song: Option<&Song>, player_info: Option<&PlayerInfo>) -> Node<Msg> {
    if let Some(ps) = song {
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
fn view_controls_down(model: &PlayerModel) -> Node<Msg> {
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

fn view_controls_up(model: &PlayerModel) -> Node<Msg> {
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
                ev(Ev::Click, |_| Msg::SendSystemCommand(
                    SystemCommand::VolDown
                ))
            ],
            button![
                C!["small-button"],
                span![C!("icon"), i![C!("fas fa-volume-up")]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolUp))
            ],
            button![
                C!["small-button"],
                span![C!["icon"], i![C!("material-icons"), shuffle]],
                ev(Ev::Click, |_| Msg::SendPlayerCommand(
                    PlayerCommand::RandomToggle
                )),
            ],
            button![
                C!["small-button"],
                span![C!["icon"], i![C!("material-icons"), audio_out]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(
                    SystemCommand::ChangeAudioOutput
                ))
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
                SystemCommand::SetVol(u8::from_str(selected.as_str()).unwrap_or_default())
            )),
        ],
    ]
}
