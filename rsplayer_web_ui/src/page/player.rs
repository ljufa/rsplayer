use api_models::common::UserCommand::Player;
use api_models::common::{MetadataCommand, PlayerCommand, SystemCommand, Volume};
use api_models::player::Song;
use api_models::state::{PlayerInfo, PlayerState, SongProgress};

use seed::{a, attrs, button, div, empty, h1, h2, h3, i, input, nav, nodes, p, prelude::*, span, style, C, IF};

use std::str::FromStr;

use crate::{Msg, PlayerModel};

// ------ ------
//     View
// ------ ------
pub fn view(model: &PlayerModel) -> Node<Msg> {
    div![
        C!["player-page"],
        div![
            C!["track-info-container", "has-background-dark-transparent"],
            view_track_info(model.current_song.as_ref(), model.player_info.as_ref()),
        ],
        view_controls(model),
    ]
}

fn view_track_info(song: Option<&Song>, player_info: Option<&PlayerInfo>) -> Node<Msg> {
    song.map_or_else(
        || empty!(),
        |ps| {
            div![
                C!["track-info", "has-text-centered"],
                h1![
                    C!["title", "is-1", "has-text-white"],
                    ps.title.as_ref().map_or("NA", |f| f)
                ],
                ps.artist.as_ref().map_or_else(
                    || h2![C!["subtitle", "is-3", "has-text-light"], "NA"],
                    |artist| a![
                        attrs!{At::Href => format!("#/library/artists?search={}", artist)},
                        h2![C!["subtitle", "is-3", "has-text-light"], artist]
                    ]
                ),
                ps.album.as_ref().map_or_else(
                    || h3![C!["subtitle", "is-5", "has-text-grey-light"], "NA"],
                    |album| a![
                        attrs!{At::Href => format!("#/library/files?search={}", album)},
                        h3![C!["subtitle", "is-5", "has-text-grey-light"], album]
                    ]
                ),
                h3![
                    C!["subtitle", "is-5", "has-text-grey-light"],
                    format!(
                        "{}",
                        player_info.map_or("NA".to_owned(), |pi| format!(
                            "{} - {} bit / {} Hz",
                            pi.codec.as_ref().map_or("", |c| c),
                            pi.audio_format_bit.map_or(0, |af| af),
                            pi.audio_format_rate.map_or(0, |r| r)
                        ))
                    )
                ],
            ]
        },
    )
}

fn view_controls(model: &PlayerModel) -> Node<Msg> {
    let playing = model.player_state == PlayerState::PLAYING;
    let shuffle_class = if model.random { "is-active" } else { "" };

    div![
        C!["player-controls", "has-text-centered"],
        div![
            C!["level", "is-mobile"],
            // Left side controls
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium", shuffle_class],
                    span![C!["icon"], i![C!["fas", "fa-shuffle"]]],
                    ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::RandomToggle))),
                ],
            ],
            // Main controls
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    span![C!["icon"], i![C!["fas", "fa-backward"]]],
                    ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::Prev))),
                ],
                button![
                    C!["button", "is-rounded", "is-large", "mx-4"],
                    span![
                        C!["icon", "is-large"],
                        i![C!["fas", if playing { "fa-pause" } else { "fa-play" }]]
                    ],
                    ev(Ev::Click, move |_| if playing {
                        Msg::SendUserCommand(Player(PlayerCommand::Pause))
                    } else {
                        Msg::SendUserCommand(Player(PlayerCommand::Play))
                    })
                ],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    span![C!["icon"], i![C!["fas", "fa-forward"]]],
                    ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::Next))),
                ],
            ],
            // Right side controls
            div![
                C!["level-item"],
                // Like button
                model.current_song.as_ref().map(|s| {
                    let id = s.file.clone();
                    let (like_class, cmd) = s.statistics.as_ref().map_or_else(
                        || ("far", MetadataCommand::LikeMediaItem(id.clone())),
                        |stat| {
                            if stat.liked_count > 0 {
                                ("fas", MetadataCommand::DislikeMediaItem(id.clone()))
                            } else {
                                ("far", MetadataCommand::LikeMediaItem(id.clone()))
                            }
                        },
                    );
                    button![
                        C!["button", "is-ghost", "is-medium"],
                        span![C!["icon"], i![C![like_class, "fa-heart"]]],
                        ev(Ev::Click, |_| Msg::LikeMediaItemClick(cmd))
                    ]
                }),
            ],
        ],
        view_track_progress_bar(&model.progress),
        view_volume_slider(&model.streamer_status.volume_state),
    ]
}

fn view_track_progress_bar(progress: &SongProgress) -> Node<Msg> {
    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        span![C!["is-size-6", "has-text-light"], progress.format_time()],
        input![
            C!["slider", "is-fullwidth", "is-large", "is-circle"],
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

fn view_volume_slider(volume_state: &Volume) -> Node<Msg> {
    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        div![
            C!["level", "is-mobile"],
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    span![C!["icon"], i![C!["fas", "fa-volume-down"]]],
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolDown))
                ],
            ],
            div![
                C!["level-item", "is-flex-grow-5"],
                input![
                    C!["slider", "is-fullwidth"],
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
            ],
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    span![C!["icon"], i![C!["fas", "fa-volume-up"]]],
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolUp))
                ],
            ],
        ],
        span![
            C!["is-size-6", "has-text-light",],
            format!("Volume: {}/{}", volume_state.current, volume_state.max)
        ],
    ]
}
