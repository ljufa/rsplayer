use api_models::common::UserCommand::Player;
use api_models::common::{MetadataCommand, PlayerCommand, SystemCommand, Volume};
use api_models::player::Song;
use api_models::state::{AudioOut, PlayerInfo, PlayerState, SongProgress};

use seed::{a, attrs, button, div, empty, i, input, nav, nodes, p, prelude::*, span, style, C, IF};

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

#[allow(clippy::too_many_lines)]
fn view_track_info(song: Option<&Song>, player_info: Option<&PlayerInfo>) -> Node<Msg> {
    song.map_or_else(
        || empty!(),
        |ps| {
            div![
                style! {
                    St::MinHeight => "300px",
                },
                C!["transparent"],
                nav![
                    C!["level", "is-flex-direction-column"],
                    IF!(ps.title.is_some() =>
                    div![
                        C!["level-item", "has-text-centered", "mb-2"],
                        div![
                            p![
                                C!["is-size-4 has-text-light has-background-dark-transparent", "has-min-width"],
                                ps.title.as_ref().map_or("NA", |f| f)
                            ],
                        ],
                    ]),
                    IF!(ps.album.is_some() =>
                    div![
                        C!["level-item", "has-text-centered", "mb-2"],
                        div![
                            p![
                                C!["has-text-light has-background-dark-transparent has-overflow-ellipsis-text-large", "has-min-width"],
                                ps.album.as_ref().map_or("NA", |f| f)
                            ],
                        ],
                    ]),
                    IF!(ps.artist.is_some() =>
                    div![
                        C!["level-item", "has-text-centered", "mb-2"],
                        div![
                            p![
                                C!["has-text-light has-background-dark-transparent has-overflow-ellipsis-text-large", "has-min-width"],
                                ps.artist.as_ref().map_or("NA", |f| f)
                            ],
                        ],
                    ]),
                    if ps.title.is_none() {
                        div![
                            C!["level-item", "has-text-centered", "mb-2"],
                            div![p![
                                C!["has-text-light has-background-dark-transparent has-overflow-ellipsis-text-large", "has-min-width"],
                                ps.file.clone()
                            ],],
                        ]
                    } else {
                        empty!()
                    },
                    IF!(ps.genre.is_some() =>
                    div![
                        C!["level-item", "has-text-centered",  "mb-2"],
                        div![
                            p![
                                C!["has-text-light has-background-dark-transparent","has-min-width"],
                                ps.genre.as_ref().map_or("NA", |f| f)
                            ],
                        ],
                    ]),
                    IF!(ps.date.is_some() =>
                    div![
                        C!["level-item", "has-text-centered"],
                        div![
                            p![
                                C!["has-text-light has-background-dark-transparent", "has-min-width"],
                                ps.date.as_ref().map_or("NA", |f| f)
                            ],
                        ],
                    ]),

                ],
                nav![
                    C!["level", "is-flex-direction-column"],
                    player_info.map_or_else(
                        || nodes!(),
                        |pi| nodes![
                            div![
                                C!["level-item", "has-text-centered", "mb-2"],
                                IF!(pi.audio_format_rate.is_some() =>
                                    div![p![
                                    C!["has-text-light has-background-dark-transparent", "has-min-width"],
                                    format!("Freq: {} | Bit: {} | Ch: {}", pi.audio_format_rate.map_or(0, |f|f),
                                    pi.audio_format_bit.map_or(0, |f|f), pi.audio_format_channels.map_or(0,|f|f))
                                ]]),
                            ],
                            pi.codec.as_ref().map(|c| {
                                div![
                                    C!["level-item", "has-text-centered", "mb-2"],
                                    div![p![C!["has-text-light has-background-dark-transparent", "has-min-width"], "Codec: ", c]],
                                ]
                            })
                        ]
                    )
                ],
            ]
        },
    )
}

fn view_track_progress_bar(progress: &SongProgress) -> Node<Msg> {
    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        span![
            C!["is-size-6", "has-text-light", "has-background-dark-transparent"],
            progress.format_time()
        ],
        input![
            C!["slider", "is-fullwidth", "is-success", "is-large", "is-circle"],
            style! {
                St::PaddingRight => "1.2rem"
            },
            attrs! {"value"=> progress.current_time.as_secs()},
            // attrs! {"step"=> 1},
            attrs! {"max"=> progress.total_time.as_secs()},
            attrs! {"min"=> 0},
            attrs! {"type"=> "range"},
            input_ev(Ev::Change, move |selected| Msg::SeekTrackPosition(
                u16::from_str(selected.as_str()).unwrap_or_default()
            )),
        ],
    ]
}
fn view_controls_down(model: &PlayerModel) -> Node<Msg> {
    let playing = model.player_state == PlayerState::PLAYING;
    div![
        C!["centered", "is-bottom"],
        a![
            C!["player-button-play", "player-button-prev",],
            ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::Prev))),
        ],
        a![
            C!["player-button-play", IF!(playing => "player-button-pause" )],
            ev(Ev::Click, move |_| if playing {
                Msg::SendUserCommand(Player(PlayerCommand::Pause))
            } else {
                Msg::SendUserCommand(Player(PlayerCommand::Play))
            })
        ],
        a![
            C!["player-button-play", "player-button-next"],
            ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::Next)))
        ],
    ]
}

fn view_controls_up(model: &PlayerModel) -> Node<Msg> {
    let audio_out = match &model.streamer_status.selected_audio_output {
        AudioOut::SPKR => "speaker",
        AudioOut::HEAD => "headset",
    };
    let shuffle = if model.random {
        "shuffle"
    } else {
        "format_list_numbered"
    };

    div![
        C!["centered", "box", "has-background-dark-transparent"],
        style! { St::Top => "28%", St::Padding => "10px", St::Width => "80%" },
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
                ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::RandomToggle))),
            ],
            button![
                C!["small-button"],
                span![C!["icon"], i![C!("material-icons"), audio_out]],
                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::ChangeAudioOutput))
            ],
            model.current_song.as_ref().map(|s| {
                let id = s.file.clone();
                let id2 = s.file.clone();
                let id3 = s.file.clone();
                let (like_class, cmd) = s.statistics.as_ref().map_or_else(
                    || ("favorite_border", MetadataCommand::LikeMediaItem(id3)),
                    |stat| {
                        if stat.liked_count > 0 {
                            ("favorite", MetadataCommand::DislikeMediaItem(id))
                        } else {
                            ("favorite_border", MetadataCommand::LikeMediaItem(id2))
                        }
                    },
                );
                button![
                    C!["small-button"],
                    span![C!["icon"], i![C!("material-icons"), like_class]],
                    ev(Ev::Click, |_| Msg::LikeMediaItemClick(cmd))
                ]
            }),
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
            // attrs! {"step"=> volume_state.step},
            attrs! {"max"=> volume_state.max},
            attrs! {"min"=> volume_state.min},
            attrs! {"type"=> "range"},
            input_ev(Ev::Change, Msg::SetVolume),
        ],
    ]
}
