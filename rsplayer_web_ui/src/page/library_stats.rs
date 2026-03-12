use api_models::common::{MetadataCommand, UserCommand};
use api_models::stat::LibraryStats;
use api_models::state::StateChangeEvent;
use seed::{attrs, div, h1, h2, p, prelude::*, progress, section, span, style, C, IF};

#[derive(Debug, Default)]
pub struct Model {
    pub stats: Option<LibraryStats>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    StatusChangeEventReceived(StateChangeEvent),
    SendUserCommand(UserCommand),
}

pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        MetadataCommand::QueryLibraryStats,
    )));
    Model::default()
}

pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::StatusChangeEventReceived(StateChangeEvent::LibraryStatsEvent(stats)) => {
            model.stats = Some(stats);
        }
        Msg::StatusChangeEventReceived(_) => {}
        Msg::SendUserCommand(cmd) => {
            orders.notify(cmd);
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    div![
        C!["library-stats"],
        section![
            C!["section"],
            style! {
                St::Background => "rgba(0,0,0,0.72)",
                St::BorderRadius => "8px",
            },
            h1![C!["title", "has-text-white"], "Library Statistics"],
            if let Some(stats) = &model.stats {
                view_stats(stats)
            } else {
                div![C!["has-text-grey"], "Loading…"]
            }
        ]
    ]
}

fn view_stats(s: &LibraryStats) -> Node<Msg> {
    let max_genre = s.top_genres.first().map_or(1, |(_, c)| *c).max(1);
    let max_decade = s.albums_by_decade.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);

    div![
        // ── Summary row ──────────────────────────────────────────────
        h2![C!["subtitle", "has-text-grey-light", "mb-2"], "Library"],
        div![
            C!["columns", "is-multiline", "mb-5"],
            stat_card("Songs", &s.total_songs.to_string(), ""),
            stat_card("Albums", &s.total_albums.to_string(), ""),
            stat_card("Artists", &s.total_artists.to_string(), ""),
            stat_card("Total duration", &format_duration(s.total_duration_secs), ""),
        ],
        // ── Playback row ─────────────────────────────────────────────
        h2![C!["subtitle", "has-text-grey-light", "mb-2"], "Playback"],
        div![
            C!["columns", "is-multiline", "mb-5"],
            stat_card("Total plays", &s.total_plays.to_string(), ""),
            stat_card("Songs played", &s.unique_songs_played.to_string(), &format!("of {}", s.total_songs)),
            stat_card("Liked songs", &s.liked_songs.to_string(), ""),
            stat_card(
                "Loudness analysed",
                &s.songs_loudness_analysed.to_string(),
                &format!("of {} songs", s.total_songs),
            ),
        ],
        // ── Loudness progress bar ─────────────────────────────────────
        IF!(s.total_songs > 0 => {
            let pct = (s.songs_loudness_analysed * 100 / s.total_songs) as u32;
            div![
                C!["mb-5"],
                p![C!["has-text-grey-light", "mb-1"], format!("Loudness analysis: {pct}%")],
                progress![
                    C!["progress", "is-info"],
                    attrs! {
                        At::Value => s.songs_loudness_analysed,
                        At::Max   => s.total_songs,
                    }
                ],
                p![
                    C!["is-size-7", "has-text-grey"],
                    "Analysis runs automatically in the background while playback is stopped. \
                     Each song is measured once (EBU R128) and the result is stored permanently."
                ],
            ]
        }),
        // ── Top genres ───────────────────────────────────────────────
        IF!(!s.top_genres.is_empty() => div![
            C!["mb-5"],
            h2![C!["subtitle", "has-text-grey-light", "mb-2"], "Top Genres"],
            s.top_genres.iter().map(|(genre, count)| {
                let pct = (*count * 100 / max_genre) as u32;
                div![
                    C!["mb-2"],
                    div![
                        C!["is-flex", "is-justify-content-space-between", "mb-1"],
                        span![C!["has-text-white"], genre],
                        span![C!["has-text-grey"], count.to_string(), " songs"],
                    ],
                    progress![
                        C!["progress", "is-primary", "is-small"],
                        attrs! { At::Value => pct, At::Max => 100u32 }
                    ],
                ]
            })
        ]),
        // ── Albums by decade ─────────────────────────────────────────
        IF!(!s.albums_by_decade.is_empty() => div![
            C!["mb-5"],
            h2![C!["subtitle", "has-text-grey-light", "mb-2"], "Albums by Decade"],
            s.albums_by_decade.iter().map(|(decade, count)| {
                let pct = (*count * 100 / max_decade) as u32;
                div![
                    C!["mb-2"],
                    div![
                        C!["is-flex", "is-justify-content-space-between", "mb-1"],
                        span![C!["has-text-white"], decade],
                        span![C!["has-text-grey"], count.to_string(), " albums"],
                    ],
                    progress![
                        C!["progress", "is-warning", "is-small"],
                        attrs! { At::Value => pct, At::Max => 100u32 }
                    ],
                ]
            })
        ]),
    ]
}

fn stat_card(label: &str, value: &str, sub: &str) -> Node<Msg> {
    div![
        C!["column", "is-3"],
        div![
            C!["box", "has-background-dark"],
            p![C!["heading", "has-text-grey"], label],
            p![C!["title", "has-text-white"], value],
            IF!(!sub.is_empty() => p![C!["has-text-grey", "is-size-7"], sub]),
        ]
    ]
}

fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}
