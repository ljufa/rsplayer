use api_models::common::{MetadataCommand, UserCommand};
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

#[component]
pub fn LibraryStatsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    use_effect(move || {
        ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryLibraryStats));
    });

    let stats = state.library_stats.read().clone();

    if let Some(s) = stats {
        let hours = s.total_duration_secs / 3600;
        let mins = (s.total_duration_secs % 3600) / 60;
        let max_genre = s.top_genres.first().map_or(1, |(_, c)| *c).max(1);
        let max_decade = s.albums_by_decade.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
        let genres: Vec<(String, usize, u32)> = s
            .top_genres
            .iter()
            .take(10)
            .map(|(g, c)| (g.clone(), *c, (*c * 100 / max_genre) as u32))
            .collect();
        let decades: Vec<(String, usize, u32)> = s
            .albums_by_decade
            .iter()
            .map(|(d, c)| (d.clone(), *c, (*c * 100 / max_decade) as u32))
            .collect();

        rsx! {
            div { class: "p-4 max-w-2xl mx-auto",
                h1 { class: "text-2xl font-bold mb-6", "Library Statistics" }

                div { class: "grid grid-cols-2 gap-3 mb-6",
                    StatCard { label: "Songs", value: s.total_songs.to_string() }
                    StatCard { label: "Albums", value: s.total_albums.to_string() }
                    StatCard { label: "Artists", value: s.total_artists.to_string() }
                    StatCard { label: "Total plays", value: s.total_plays.to_string() }
                    StatCard { label: "Liked songs", value: s.liked_songs.to_string() }
                    StatCard { label: "Analysed", value: s.songs_loudness_analysed.to_string() }
                }

                div { class: "bg-base-200 rounded-lg p-4 mb-6",
                    p { class: "text-sm text-base-content/60 mb-1", "Total duration" }
                    p { class: "text-xl font-bold", "{hours}h {mins}m" }
                }

                if !genres.is_empty() {
                    div { class: "mb-6",
                        h2 { class: "text-lg font-semibold mb-3", "Top Genres" }
                        {genres.into_iter().map(|(genre, count, pct)| rsx! {
                            div { class: "mb-2",
                                div { class: "flex justify-between text-sm mb-1",
                                    span { "{genre}" }
                                    span { class: "text-base-content/50", "{count}" }
                                }
                                div { class: "w-full bg-base-300 rounded-full h-2",
                                    div { class: "bg-primary h-2 rounded-full", style: "width:{pct}%" }
                                }
                            }
                        })}
                    }
                }

                if !decades.is_empty() {
                    div { class: "mb-6",
                        h2 { class: "text-lg font-semibold mb-3", "Albums by Decade" }
                        {decades.into_iter().map(|(decade, count, pct)| rsx! {
                            div { class: "mb-2",
                                div { class: "flex justify-between text-sm mb-1",
                                    span { "{decade}" }
                                    span { class: "text-base-content/50", "{count} albums" }
                                }
                                div { class: "w-full bg-base-300 rounded-full h-2",
                                    div { class: "bg-secondary h-2 rounded-full", style: "width:{pct}%" }
                                }
                            }
                        })}
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "p-4 max-w-2xl mx-auto",
                h1 { class: "text-2xl font-bold mb-6", "Library Statistics" }
                div { class: "flex items-center gap-3 text-base-content/50 py-8",
                    span { class: "loading loading-spinner" }
                    span { "Loading statistics..." }
                }
            }
        }
    }
}

#[component]
fn StatCard(label: String, value: String) -> Element {
    rsx! {
        div { class: "bg-base-200 rounded-lg p-3",
            p { class: "text-xs text-base-content/50 mb-1", "{label}" }
            p { class: "text-xl font-bold", "{value}" }
        }
    }
}
