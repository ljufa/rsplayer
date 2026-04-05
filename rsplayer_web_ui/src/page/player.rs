use api_models::common::UserCommand::Player;
use api_models::common::{MetadataCommand, PlaybackMode, PlayerCommand, SystemCommand, Volume};
use api_models::player::Song;
use api_models::state::{PlayerInfo, PlayerState, SongProgress};

use seed::{a, attrs, button, canvas, div, empty, h1, h2, h3, i, id, input, p, prelude::*, span, style, C, IF};

use std::str::FromStr;

use crate::{Msg, PlayerModel};

// ------ ------
//     View
// ------ ------
pub fn view(model: &PlayerModel) -> Node<Msg> {
    div![
        C!["player-page"],
        // Visualizer — absolute background layer, covers full player-page
        IF!(model.vu_meter_enabled => div![
            style! {
                St::Position => "absolute",
                St::Top => "0",
                St::Left => "0",
                St::Width => "100%",
                St::Height => "100%",
                St::ZIndex => "0",
                St::PointerEvents => "none",
            },
            canvas![
                id!("vumeter"),
                style! {
                    St::Display => "block",
                    St::Width => "100%",
                    St::Height => "100%",
                }
            ],
        ]),
        div![
            C!["track-info-container", "has-background-dark-transparent"],
            view_track_info(model.current_song.as_ref(), model.player_info.as_ref()),
        ],
        view_controls(model),
        IF!(model.lyrics_modal_open => view_lyrics_modal(model)),
    ]
}

fn view_lyrics_modal(model: &PlayerModel) -> Node<Msg> {
    // Offset calculation using the actual ring buffer size in milliseconds.
    let latency_offset = model.ring_buffer_size_ms as f64 / 1000.0;
    let current_time = (model.progress.current_time.as_secs_f64() - latency_offset).max(0.0);

    let active_index = model
        .parsed_lyrics
        .as_ref()
        .and_then(|lines| lines.iter().rposition(|line| line.time_secs <= current_time));

    div![
        C!["modal", "is-active"],
        div![C!["modal-background"], ev(Ev::Click, |_| Msg::ToggleLyricsModal)],
        div![
            C!["modal-content"],
            id!("lyrics-modal-content"),
            style! {
                St::BackgroundColor => "#1a1a1a",
                St::Color => "white",
                St::Padding => "2rem",
                St::BorderRadius => "8px",
                St::MaxHeight => "80vh",
                St::OverflowY => "auto",
                St::Width => "90%",
                St::MaxWidth => "600px",
            },
            if model.lyrics_loading {
                div![C!["has-text-centered"], "Loading lyrics..."]
            } else if let Some(lyrics) = &model.parsed_lyrics {
                div![
                    C!["lyrics-list"],
                    lyrics.iter().enumerate().map(|(idx, line)| {
                        let is_active = Some(idx) == active_index;
                        div![
                            IF!(is_active => id!("lyric-active")),
                            style! {
                                St::FontSize => "1.25rem",
                                St::FontWeight => if is_active { "700" } else { "400" },
                                St::Color => if is_active { "white" } else { "#888" },
                                St::Padding => "0.6rem 0",
                                St::Transition => "all 0.3s ease",
                                St::TextAlign => "center",
                                St::LineHeight => "1.4",
                            },
                            line.text.clone()
                        ]
                    })
                ]
            } else if let Some(lyrics) = &model.lyrics {
                if let Some(plain) = &lyrics.plain_lyrics {
                    div![
                        style! {
                            St::WhiteSpace => "pre-wrap",
                            St::TextAlign => "center",
                            St::FontSize => "1.2rem",
                        },
                        plain.clone()
                    ]
                } else {
                    div![C!["has-text-centered"], "Lyrics not found."]
                }
            } else {
                div![C!["has-text-centered"], "Lyrics not found."]
            }
        ],
        button![
            C!["modal-close", "is-large"],
            attrs! {At::AriaLabel => "close"},
            ev(Ev::Click, |_| Msg::ToggleLyricsModal)
        ]
    ]
}

fn view_track_info(song: Option<&Song>, player_info: Option<&PlayerInfo>) -> Node<Msg> {
    song.map_or_else(view_skeleton_track_info, |ps| {
        div![
            C!["track-info", "has-text-centered"],
            h1![
                C![
                    "title",
                    "has-text-white",
                    match ps.title.as_ref().map_or(0, |t| t.len()) {
                        0..=19 => "is-1",
                        20..=31 => "is-2",
                        _ => "is-3",
                    }
                ],
                ps.title.as_ref().map_or("Unknown Track", |f| f)
            ],
            ps.artist.as_ref().map_or_else(
                || empty!(),
                |artist| a![
                    style! { St::TextDecoration => "underline" },
                    attrs! {At::Href => format!("#/library/artists?search={}", artist)},
                    h2![C!["subtitle", "is-3", "has-text-light"], artist]
                ]
            ),
            ps.album.as_ref().map_or_else(
                || empty!(),
                |album| a![
                    style! { St::TextDecoration => "underline" },
                    attrs! {At::Href => format!("#/library/files?search={}", album)},
                    h3![C!["subtitle", "is-5", "has-text-grey-light"], album]
                ]
            ),
            ps.genre.as_ref().map_or_else(
                || empty!(),
                |genre| h3![C!["subtitle", "is-5", "has-text-grey-light"], genre],
            ),
            ps.date.as_ref().map_or_else(
                || empty!(),
                |date| h3![C!["subtitle", "is-5", "has-text-grey-light"], date],
            ),
            h3![
                C!["subtitle", "is-5", "has-text-grey-light"],
                player_info.map_or("No file playing".to_owned(), |pi| format!(
                    "{} - {} / {} Hz",
                    pi.codec.as_ref().map_or("", |c| c),
                    pi.audio_format_bit.map_or(0, |af| af),
                    pi.audio_format_rate.map_or(0, |r| r)
                ))
            ],
            IF!(player_info.and_then(|pi| pi.track_loudness_lufs.or(pi.normalization_gain_db)).is_some() =>
                h3![
                    C!["subtitle", "is-6", "has-text-grey-light"],
                    {
                        let pi = player_info.unwrap();
                        match (pi.track_loudness_lufs, pi.normalization_gain_db) {
                            (Some(lufs_hundredths), Some(gain_hundredths)) => {
                                let lufs = lufs_hundredths as f64 / 100.0;
                                let gain = gain_hundredths as f64 / 100.0;
                                let effective = lufs + gain;
                                format!("{lufs:.1} LUFS  →  {gain:+.1} dB  →  {effective:.1} LUFS")
                            }
                            (Some(lufs_hundredths), None) => {
                                let lufs = lufs_hundredths as f64 / 100.0;
                                format!("{lufs:.1} LUFS")
                            }
                            (None, Some(gain_hundredths)) => {
                                let gain = gain_hundredths as f64 / 100.0;
                                format!("{gain:+.1} dB (file tag)")
                            }
                            (None, None) => String::new(),
                        }
                    }
                ]
            ),
        ]
    })
}

fn view_skeleton_track_info() -> Node<Msg> {
    div![
        C!["skeleton-player"],
        div![C!["skeleton skeleton-player-image"]],
        div![C!["skeleton skeleton-player-title"]],
        div![C!["skeleton skeleton-player-artist"]],
        p![
            C!["has-text-grey-light"],
            style! { St::FontSize => "0.9rem", St::MarginTop => "20px" },
            "Ready to play - add songs from your library"
        ],
    ]
}

fn view_controls(model: &PlayerModel) -> Node<Msg> {
    let playing = model.player_state == PlayerState::PLAYING;
    let (shuffle_class, shuffle_title) = match model.playback_mode {
        PlaybackMode::Sequential => ("fa-list-ol", "Sequential Playback"),
        PlaybackMode::Random => ("fa-shuffle", "Random Playback"),
        PlaybackMode::LoopSingle => ("fa-repeat", "Loop Single Song"),
        PlaybackMode::LoopQueue => ("fa-arrows-rotate", "Loop Queue"),
    };

    div![
        C!["player-controls", "has-text-centered"],
        div![
            C!["level", "is-mobile"],
            // Left side controls
            div![
                C!["level-item"],
                // Visualizer toggle (only shown when Music Visualization is enabled)
                IF!(model.vu_meter_enabled => button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => "Toggle visualizer (V)"},
                    span![C!["icon"], i![C!["fas", "fa-chart-bar"]]],
                    ev(Ev::Click, |_| Msg::ToggleVisualizer)
                ]),
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => format!("{} (S)", shuffle_title)},
                    span![C!["icon"], i![C!["fas", shuffle_class]]],
                    ev(Ev::Click, |_| Msg::SendUserCommand(Player(
                        PlayerCommand::CyclePlaybackMode
                    ))),
                ],
            ],
            // Main controls
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => "Previous (←)"},
                    span![C!["icon"], i![C!["fas", "fa-backward"]]],
                    ev(Ev::Click, |_| Msg::SendUserCommand(Player(PlayerCommand::Prev))),
                ],
                button![
                    C!["button", "is-rounded", "is-large", "mx-4"],
                    attrs! {At::Title => if playing { "Pause (Space)" } else { "Play (Space)" }},
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
                    attrs! {At::Title => "Next (→)"},
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
                        attrs! {At::Title => "Like / Unlike (L)"},
                        span![C!["icon"], i![C![like_class, "fa-heart"]]],
                        ev(Ev::Click, |_| Msg::LikeMediaItemClick(cmd))
                    ]
                }),
                // Lyrics button
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => "Lyrics (Y)"},
                    span![C!["icon"], i![C!["fas", "fa-align-left"]]],
                    ev(Ev::Click, |_| Msg::ToggleLyricsModal)
                ],
            ],
        ],
        view_track_progress_bar(&model.progress),
        view_volume_slider(&model.volume_state, model.volume_state.current),
    ]
}

fn view_track_progress_bar(progress: &SongProgress) -> Node<Msg> {
    let current_secs = progress.current_time.as_secs();
    let total_secs = progress.total_time.as_secs();
    let current_formatted = format_time(current_secs);
    let total_formatted = format_time(total_secs);

    div![
        C!["progress-bar-container"],
        style! {
            St::Padding => "1.2rem",
        },
        // Time display row
        div![
            C!["level", "is-mobile"],
            style! { St::MarginBottom => "0.5rem" },
            div![
                C!["level-item", "is-justify-content-flex-start"],
                span![
                    C!["is-size-6", "has-text-light", "progress-time-current"],
                    current_formatted
                ],
            ],
            div![
                C!["level-item", "is-justify-content-flex-end"],
                span![
                    C!["is-size-6", "has-text-light", "progress-time-total"],
                    total_formatted
                ],
            ],
        ],
        // Progress slider wrapper for tooltip
        div![
            C!["progress-slider-wrapper"],
            // Tooltip (shows on hover via CSS)
            div![C!["progress-tooltip"], id!("progress-tooltip"), "0:00"],
            input![
                C![
                    "slider",
                    "is-fullwidth",
                    "is-large",
                    "is-circle",
                    "player-progress-slider"
                ],
                style! {
                    St::PaddingRight => "1.2rem"
                },
                attrs! {"value"=> current_secs},
                attrs! {"max"=> total_secs},
                attrs! {"min"=> 0},
                attrs! {"type"=> "range"},
                attrs! {"aria-label"=> "Track progress"},
                input_ev(Ev::Input, move |selected| Msg::SeekTrackPositionInput(
                    u16::from_str(selected.as_str()).unwrap_or_default()
                )),
                input_ev(Ev::Change, move |selected| Msg::SeekTrackPosition(
                    u16::from_str(selected.as_str()).unwrap_or_default()
                )),
            ],
        ],
    ]
}

// Helper function to format seconds to MM:SS
fn format_time(seconds: u64) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{}:{:02}", mins, secs)
}

fn view_volume_slider(volume_state: &Volume, current_volume: u8) -> Node<Msg> {
    let is_muted = current_volume == 0;
    let max_vol = volume_state.max;
    let volume_percent = if max_vol > 0 {
        (current_volume as f32 / max_vol as f32 * 100.0) as u8
    } else {
        0
    };

    div![
        style! {
            St::Padding => "1.2rem",
        },
        C!["has-text-centered"],
        div![
            C!["level", "is-mobile", "volume-control-row"],
            // Mute toggle button
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => if is_muted { "Unmute (M)" } else { "Mute (M)" }},
                    span![
                        C!["icon"],
                        i![C!["fas", if is_muted { "fa-volume-xmark" } else { "fa-volume-off" }]]
                    ],
                    ev(Ev::Click, |_| Msg::ToggleMute)
                ],
            ],
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => "Volume down (↓)"},
                    span![C!["icon"], i![C!["fas", "fa-circle-minus"]]],
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolDown))
                ],
            ],
            div![
                C!["level-item", "is-flex-grow-5", "volume-slider-wrapper"],
                // Volume percentage tooltip
                div![
                    C!["volume-tooltip"],
                    id!("volume-tooltip"),
                    format!("{}%", volume_percent)
                ],
                input![
                    C!["slider", "is-fullwidth", "player-volume-slider"],
                    style! {
                        St::PaddingRight => "1.2rem"
                    },
                    attrs! {"value"=> current_volume},
                    attrs! {"max"=> max_vol},
                    attrs! {"min"=> 0},
                    attrs! {"type"=> "range"},
                    attrs! {"aria-label"=> "Volume"},
                    input_ev(Ev::Input, Msg::SetVolumeInput),
                    input_ev(Ev::Change, Msg::SetVolume),
                ],
            ],
            div![
                C!["level-item"],
                button![
                    C!["button", "is-ghost", "is-medium"],
                    attrs! {At::Title => "Volume up (↑)"},
                    span![C!["icon"], i![C!["fas", "fa-circle-plus"]]],
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::VolUp))
                ],
            ],
        ],
        // Volume percentage display
        span![
            C!["is-size-6", "has-text-light", "volume-percentage-display"],
            format!("Volume: {}%", volume_percent)
        ],
    ]
}
