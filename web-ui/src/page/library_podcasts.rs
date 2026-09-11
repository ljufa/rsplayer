//! Library → Podcasts: subscriptions grid, per-show episode list with
//! progress/played state, and directory search / subscribe-by-URL.
//!
//! All data comes over the WebSocket (`PodcastCommand` → `Podcast*Event`s
//! in `AppState`); the page itself only holds view state.

use api_models::common::UserCommand;
use api_models::podcast::{Episode, Podcast, PodcastCommand, PodcastSearchResult};
use dioxus::prelude::*;
use web_sys::WebSocket;

use crate::{CurrentPath, hooks::ws_send, navigate, state::AppState};

const PAGE_SIZE: usize = 50;
const PLACEHOLDER_IMAGE: &str = "/no_album.svg";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Subscriptions,
    Search,
}

fn podcast_cmd(ws: &Signal<Option<WebSocket>>, cmd: PodcastCommand) {
    ws_send(ws, &UserCommand::Podcast(cmd));
}

/// `?podcast=<id>` from the current location, for deep links from the player.
fn route_podcast_id() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|s| {
            s.trim_start_matches('?')
                .split('&')
                .find_map(|p| p.strip_prefix("podcast=").map(ToString::to_string))
        })
        .filter(|id| !id.is_empty())
}

/// `1h 02m` / `42m` / `0:58` style duration.
pub fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("0:{s:02}")
    }
}

/// Meta line under an episode title: date • duration • "12m left".
pub fn episode_meta(ep: &Episode) -> String {
    let mut parts = Vec::new();
    if let Some(date) = ep.published {
        parts.push(date.format("%Y-%m-%d").to_string());
    }
    if let Some(total) = ep.duration_secs {
        parts.push(fmt_duration(total));
        if ep.position_secs > 0 && !ep.played && total > ep.position_secs {
            parts.push(format!("{} left", fmt_duration(total - ep.position_secs)));
        }
    } else if ep.position_secs > 0 && !ep.played {
        parts.push(format!("at {}", fmt_duration(ep.position_secs)));
    }
    parts.join(" • ")
}

fn is_feed_url(input: &str) -> bool {
    let t = input.trim();
    t.starts_with("http://") || t.starts_with("https://")
}

#[component]
pub fn LibraryPodcastsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let CurrentPath(path) = use_context::<CurrentPath>();

    let mut tab = use_signal(|| Tab::Subscriptions);
    let mut selected: Signal<Option<String>> = use_signal(route_podcast_id);
    let mut search = use_signal(String::new);
    let mut searched = use_signal(|| false);
    let mut confirm_unsubscribe = use_signal(|| false);
    let mut expanded_episode: Signal<Option<String>> = use_signal(|| None);

    // Subscriptions on mount (and after a reconnect).
    use_effect(move || {
        if ws.read().is_some() {
            podcast_cmd(&ws, PodcastCommand::QueryPodcasts);
        }
    });

    // First episode page of the selected show, unless already cached.
    use_effect(move || {
        if let Some(id) = selected() {
            let cached = state.podcast_episodes.peek().contains_key(&id);
            if !cached && ws.read().is_some() {
                podcast_cmd(
                    &ws,
                    PodcastCommand::QueryEpisodes {
                        podcast_id: id,
                        offset: 0,
                        limit: PAGE_SIZE,
                    },
                );
            }
        }
    });

    let mut open_podcast = move |id: String| {
        confirm_unsubscribe.set(false);
        expanded_episode.set(None);
        navigate(path, &format!("/library/podcasts?podcast={id}"));
        selected.set(Some(id));
    };

    let mut run_search = move || {
        let term = search().trim().to_string();
        if term.is_empty() {
            return;
        }
        if is_feed_url(&term) {
            podcast_cmd(&ws, PodcastCommand::Subscribe { feed_url: term });
            search.set(String::new());
            tab.set(Tab::Subscriptions);
        } else {
            searched.set(true);
            podcast_cmd(&ws, PodcastCommand::Search(term));
        }
    };

    let busy = (state.podcast_busy)();
    let podcasts = state.podcasts.read().clone();
    let selected_podcast = selected().and_then(|id| podcasts.iter().find(|p| p.id == id).cloned());

    rsx! {
        div { class: "library-page",
            // ── Tabs ──────────────────────────────────────────────────────
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300 overflow-x-auto",
                for (label , t) in [("Subscriptions", Tab::Subscriptions), ("Search", Tab::Search)] {
                    button {
                        key: "{label}",
                        class: if tab() == t { "btn btn-sm btn-primary" } else { "btn btn-sm btn-ghost" },
                        onclick: move |_| {
                            tab.set(t);
                            if t == Tab::Subscriptions {
                                selected.set(None);
                            }
                        },
                        "{label}"
                    }
                }
                div { class: "flex-1" }
                if busy {
                    span { class: "loading loading-spinner loading-sm text-primary", title: "Working…" }
                }
                if tab() == Tab::Subscriptions && !podcasts.is_empty() {
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Refresh all feeds",
                        disabled: busy,
                        onclick: move |_| podcast_cmd(&ws, PodcastCommand::Refresh(None)),
                        i { class: "material-icons text-base", "refresh" }
                    }
                }
            }

            match tab() {
                Tab::Search => rsx! {
                    div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300",
                        input {
                            class: "input input-sm input-bordered flex-1",
                            r#type: "text",
                            placeholder: "Search podcasts, or paste a feed URL…",
                            value: "{search}",
                            autofocus: true,
                            oninput: move |e| search.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    run_search();
                                }
                            },
                        }
                        button {
                            class: "btn btn-sm btn-primary",
                            title: if is_feed_url(&search()) { "Subscribe to this feed" } else { "Search" },
                            disabled: busy || search().trim().is_empty(),
                            onclick: move |_| run_search(),
                            i { class: "material-icons text-base",
                                if is_feed_url(&search()) { "add" } else { "search" }
                            }
                        }
                        button {
                            class: "btn btn-sm btn-ghost",
                            title: "Clear",
                            onclick: move |_| {
                                search.set(String::new());
                                searched.set(false);
                                state.podcast_search_results.clone().set(Vec::new());
                            },
                            i { class: "material-icons text-base", "backspace" }
                        }
                    }
                    SearchResults {
                        results: state.podcast_search_results.read().clone(),
                        subscribed_feeds: podcasts.iter().map(|p| p.feed_url.clone()).collect::<Vec<_>>(),
                        searched: searched(),
                        busy,
                        on_subscribe: move |feed_url: String| podcast_cmd(&ws, PodcastCommand::Subscribe { feed_url }),
                    }
                },
                Tab::Subscriptions => match selected_podcast {
                    Some(podcast) => rsx! {
                        PodcastHeader {
                            podcast: podcast.clone(),
                            busy,
                            confirm_unsubscribe: confirm_unsubscribe(),
                            on_back: move |_| {
                                selected.set(None);
                                navigate(path, "/library/podcasts");
                            },
                            on_refresh: {
                                let id = podcast.id.clone();
                                move |_| podcast_cmd(&ws, PodcastCommand::Refresh(Some(id.clone())))
                            },
                            on_unsubscribe: {
                                let id = podcast.id.clone();
                                move |_| {
                                    if confirm_unsubscribe() {
                                        podcast_cmd(&ws, PodcastCommand::Unsubscribe(id.clone()));
                                        confirm_unsubscribe.set(false);
                                        selected.set(None);
                                        navigate(path, "/library/podcasts");
                                    } else {
                                        confirm_unsubscribe.set(true);
                                    }
                                }
                            },
                        }
                        EpisodeList {
                            podcast: podcast.clone(),
                            expanded: expanded_episode(),
                            on_toggle_expand: move |id: String| {
                                let current = expanded_episode();
                                expanded_episode.set(if current.as_deref() == Some(id.as_str()) { None } else { Some(id) });
                            },
                        }
                    },
                    None => rsx! {
                        if podcasts.is_empty() {
                            div { class: "flex flex-col items-center py-16 gap-3 text-base-content/40",
                                i { class: "material-icons text-5xl", "podcasts" }
                                p { class: "text-center", "No podcast subscriptions yet." }
                                p { class: "text-sm text-center px-6",
                                    "Use Search to find shows by name, or paste an RSS feed URL."
                                }
                                button {
                                    class: "btn btn-sm btn-primary mt-2",
                                    onclick: move |_| tab.set(Tab::Search),
                                    i { class: "material-icons text-base", "search" }
                                    "Find podcasts"
                                }
                            }
                        } else {
                            div { class: "grid grid-cols-3 sm:grid-cols-4 gap-3 p-3",
                                for podcast in podcasts.iter().cloned() {
                                    {
                                        let id = podcast.id.clone();
                                        let img = podcast.image_url.clone().unwrap_or_else(|| PLACEHOLDER_IMAGE.to_string());
                                        let author = podcast.author.clone().unwrap_or_default();
                                        let has_error = podcast.last_error.is_some();
                                        rsx! {
                                            div {
                                                key: "{podcast.id}",
                                                class: "card bg-base-200 cursor-pointer hover:bg-base-300 transition shadow-sm",
                                                onclick: move |_| open_podcast(id.clone()),
                                                figure { class: "relative aspect-square overflow-hidden rounded-t-lg bg-base-300",
                                                    img { class: "w-full h-full object-cover", src: "{img}", alt: "{podcast.title}", loading: "lazy" }
                                                    if podcast.unplayed_count > 0 {
                                                        span { class: "badge badge-primary badge-sm absolute top-1 right-1", "{podcast.unplayed_count}" }
                                                    }
                                                    if has_error {
                                                        span { class: "absolute bottom-1 left-1 text-error", title: "Last refresh failed",
                                                            i { class: "material-icons text-base", "error_outline" }
                                                        }
                                                    }
                                                }
                                                div { class: "card-body p-2",
                                                    p { class: "text-xs font-semibold leading-tight line-clamp-2", "{podcast.title}" }
                                                    if !author.is_empty() {
                                                        p { class: "text-xs text-base-content/50 truncate", "{author}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                },
            }
        }
    }
}

#[component]
fn PodcastHeader(
    podcast: Podcast,
    busy: bool,
    confirm_unsubscribe: bool,
    on_back: EventHandler<MouseEvent>,
    on_refresh: EventHandler<MouseEvent>,
    on_unsubscribe: EventHandler<MouseEvent>,
) -> Element {
    let img = podcast.image_url.clone().unwrap_or_else(|| PLACEHOLDER_IMAGE.to_string());
    let author = podcast.author.clone().unwrap_or_default();
    let refreshed = podcast
        .last_refreshed
        .map(|t| format!("Updated {}", t.format("%Y-%m-%d %H:%M")))
        .unwrap_or_default();
    rsx! {
        div { class: "flex items-start gap-3 px-3 py-3 border-b border-base-300",
            button { class: "btn btn-ghost btn-sm btn-circle", title: "Back", onclick: move |e| on_back.call(e),
                i { class: "material-icons", "arrow_back" }
            }
            img { class: "w-16 h-16 rounded object-cover bg-base-300 shrink-0", src: "{img}", alt: "{podcast.title}" }
            div { class: "flex-1 min-w-0",
                p { class: "font-semibold leading-tight line-clamp-2", "{podcast.title}" }
                if !author.is_empty() {
                    p { class: "text-sm text-base-content/60 truncate", "{author}" }
                }
                p { class: "text-xs text-base-content/40 truncate",
                    "{podcast.episode_count} episodes"
                    if podcast.unplayed_count > 0 { " • {podcast.unplayed_count} unplayed" }
                    if !refreshed.is_empty() { " • {refreshed}" }
                }
                if let Some(err) = podcast.last_error.clone() {
                    p { class: "text-xs text-error truncate", title: "{err}", "Last refresh failed: {err}" }
                }
                if let Some(site) = podcast.website.clone() {
                    a { class: "text-xs link link-hover text-base-content/50", href: "{site}", target: "_blank", rel: "noopener", "Website" }
                }
            }
            div { class: "flex flex-col gap-1 shrink-0",
                button {
                    class: "btn btn-ghost btn-xs",
                    title: "Refresh feed",
                    disabled: busy,
                    onclick: move |e| on_refresh.call(e),
                    i { class: "material-icons text-base", "refresh" }
                }
                button {
                    class: if confirm_unsubscribe { "btn btn-error btn-xs" } else { "btn btn-ghost btn-xs text-error" },
                    title: if confirm_unsubscribe { "Click again to confirm" } else { "Unsubscribe" },
                    onclick: move |e| on_unsubscribe.call(e),
                    if confirm_unsubscribe {
                        "Confirm"
                    } else {
                        i { class: "material-icons text-base", "delete_outline" }
                    }
                }
            }
        }
    }
}

#[component]
fn EpisodeList(podcast: Podcast, expanded: Option<String>, on_toggle_expand: EventHandler<String>) -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let page = state.podcast_episodes.read().get(&podcast.id).cloned();
    let podcast_id = podcast.id.clone();

    let Some(page) = page else {
        return rsx! {
            div { class: "flex flex-col gap-1 p-3",
                {(0..6).map(|i| rsx! {
                    div { key: "{i}", class: "flex items-center gap-2 py-1.5 px-2",
                        div { class: "skeleton w-10 h-10 rounded" }
                        div { class: "flex-1 flex flex-col gap-1",
                            div { class: "skeleton h-4 w-3/4 rounded" }
                            div { class: "skeleton h-3 w-1/2 rounded" }
                        }
                    }
                })}
            }
        };
    };

    if page.episodes.is_empty() {
        return rsx! {
            div { class: "flex flex-col items-center py-12 gap-2 text-base-content/40",
                i { class: "material-icons text-4xl", "podcasts" }
                p { "No episodes with audio found in this feed." }
            }
        };
    }

    let loaded = page.episodes.len();
    let total = page.total;
    let show_image = podcast.image_url.clone();

    rsx! {
        div { class: "flex flex-col",
            for ep in page.episodes.iter().cloned() {
                EpisodeRow {
                    key: "{ep.id}",
                    episode: ep.clone(),
                    fallback_image: show_image.clone(),
                    expanded: expanded.as_deref() == Some(ep.id.as_str()),
                    on_toggle_expand: move |id| on_toggle_expand.call(id),
                }
            }
            if loaded < total {
                div { class: "px-3 py-2",
                    button {
                        class: "btn btn-outline btn-primary btn-sm w-full",
                        onclick: move |_| podcast_cmd(
                            &ws,
                            PodcastCommand::QueryEpisodes {
                                podcast_id: podcast_id.clone(),
                                offset: loaded,
                                limit: PAGE_SIZE,
                            },
                        ),
                        "Load more ({loaded} of {total})"
                    }
                }
            }
        }
    }
}

#[component]
fn EpisodeRow(episode: Episode, fallback_image: Option<String>, expanded: bool, on_toggle_expand: EventHandler<String>) -> Element {
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let img = episode
        .image_url
        .clone()
        .or(fallback_image)
        .unwrap_or_else(|| PLACEHOLDER_IMAGE.to_string());
    let meta = episode_meta(&episode);
    let progress_pct = episode.progress_fraction().map(|f| (f * 100.0).round() as u32);
    let in_progress = !episode.played && episode.position_secs > 0;
    let played = episode.played;
    let id = episode.id.clone();
    let description = episode.description.clone().unwrap_or_default();
    let row_class = if played {
        "flex items-start gap-2 sm:gap-3 px-3 py-2 hover:bg-base-200 group opacity-50"
    } else {
        "flex items-start gap-2 sm:gap-3 px-3 py-2 hover:bg-base-200 group"
    };
    let play_title = if in_progress { "Resume" } else { "Play now" };
    let played_title = if played { "Mark as unplayed" } else { "Mark as played" };
    let played_icon = if played { "remove_done" } else { "done" };

    // Each handler owns its own copy of the id.
    let play = {
        let id = id.clone();
        move |_| podcast_cmd(&ws, PodcastCommand::PlayEpisode(id.clone()))
    };
    let next = |id: String| move |_| podcast_cmd(&ws, PodcastCommand::AddEpisodeAfterCurrent(id.clone()));
    let add = |id: String| move |_| podcast_cmd(&ws, PodcastCommand::AddEpisodeToQueue(id.clone()));
    let toggle_played = |id: String| move |_| podcast_cmd(&ws, PodcastCommand::SetPlayed(id.clone(), !played));
    let toggle_expand = |id: String| move |_| on_toggle_expand.call(id.clone());

    rsx! {
        div { class: "border-b border-base-300/50",
            div { class: "{row_class}",
                img { class: "w-10 h-10 rounded object-cover bg-base-300 shrink-0 mt-0.5", src: "{img}", alt: "", loading: "lazy" }
                div {
                    class: "flex-1 min-w-0 cursor-pointer",
                    onclick: toggle_expand(id.clone()),
                    p { class: if expanded { "text-sm font-medium" } else { "text-sm font-medium line-clamp-2" },
                        if played {
                            i { class: "material-icons text-sm align-middle mr-1 text-success", "check_circle" }
                        }
                        "{episode.title}"
                    }
                    if !meta.is_empty() {
                        p { class: "text-xs text-base-content/50 truncate", "{meta}" }
                    }
                    if in_progress {
                        if let Some(pct) = progress_pct {
                            progress { class: "progress progress-primary h-1 w-full mt-1", value: "{pct}", max: "100" }
                        }
                    }
                }
                div { class: "flex items-center gap-0.5 shrink-0",
                    // Desktop: secondary actions appear on hover. Phones get
                    // them from the expanded panel so the title keeps its width.
                    div { class: "hidden sm:group-hover:flex items-center gap-0.5",
                        button { class: "btn btn-ghost btn-xs btn-square", title: "Play next", onclick: next(id.clone()),
                            i { class: "material-icons text-sm", "queue_play_next" }
                        }
                        button { class: "btn btn-ghost btn-xs btn-square", title: "Add to queue", onclick: add(id.clone()),
                            i { class: "material-icons text-sm", "playlist_add" }
                        }
                        button { class: "btn btn-ghost btn-xs btn-square", title: "{played_title}", onclick: toggle_played(id.clone()),
                            i { class: "material-icons text-sm", "{played_icon}" }
                        }
                    }
                    button { class: "btn btn-ghost btn-sm btn-square", title: "{play_title}", onclick: play,
                        i { class: "material-icons", "play_arrow" }
                    }
                    button {
                        class: "btn btn-ghost btn-sm btn-square sm:hidden",
                        title: if expanded { "Less" } else { "More actions" },
                        onclick: toggle_expand(id.clone()),
                        i { class: "material-icons", if expanded { "expand_less" } else { "more_vert" } }
                    }
                }
            }
            if expanded {
                div { class: "px-3 pb-3 sm:pl-16 flex flex-col gap-2",
                    div { class: "flex flex-wrap gap-1",
                        button { class: "btn btn-xs btn-outline", onclick: next(id.clone()),
                            i { class: "material-icons text-sm", "queue_play_next" }
                            "Play next"
                        }
                        button { class: "btn btn-xs btn-outline", onclick: add(id.clone()),
                            i { class: "material-icons text-sm", "playlist_add" }
                            "Add to queue"
                        }
                        button { class: "btn btn-xs btn-outline", onclick: toggle_played(id.clone()),
                            i { class: "material-icons text-sm", "{played_icon}" }
                            "{played_title}"
                        }
                    }
                    if !description.is_empty() {
                        p { class: "text-xs text-base-content/70 whitespace-pre-line", "{description}" }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchResults(
    results: Vec<PodcastSearchResult>,
    subscribed_feeds: Vec<String>,
    searched: bool,
    busy: bool,
    on_subscribe: EventHandler<String>,
) -> Element {
    if busy && results.is_empty() {
        return rsx! {
            div { class: "flex flex-col gap-1 p-3",
                {(0..6).map(|i| rsx! {
                    div { key: "{i}", class: "flex items-center gap-2 py-1.5 px-2",
                        div { class: "skeleton w-12 h-12 rounded" }
                        div { class: "flex-1 flex flex-col gap-1",
                            div { class: "skeleton h-4 w-3/4 rounded" }
                            div { class: "skeleton h-3 w-1/2 rounded" }
                        }
                    }
                })}
            }
        };
    }
    if results.is_empty() {
        return rsx! {
            div { class: "flex flex-col items-center py-16 gap-3 text-base-content/40",
                i { class: "material-icons text-5xl", "travel_explore" }
                if searched {
                    p { "No podcasts found. Try a different search term." }
                } else {
                    p { class: "text-center px-6", "Search by show name, or paste an RSS feed URL to subscribe directly." }
                }
            }
        };
    }
    rsx! {
        div { class: "flex flex-col",
            for r in results.iter().cloned() {
                {
                    let img = r.image_url.clone().unwrap_or_else(|| PLACEHOLDER_IMAGE.to_string());
                    let author = r.author.clone().unwrap_or_default();
                    let subscribed = subscribed_feeds.contains(&r.feed_url);
                    let mut details = r.categories.iter().take(3).cloned().collect::<Vec<_>>();
                    if let Some(n) = r.episode_count {
                        details.push(format!("{n} episodes"));
                    }
                    let details = details.join(" • ");
                    let feed_url = r.feed_url.clone();
                    rsx! {
                        div { key: "{r.feed_url}", class: "flex items-center gap-3 px-3 py-2 hover:bg-base-200 border-b border-base-300/50",
                            img { class: "w-12 h-12 rounded object-cover bg-base-300 shrink-0", src: "{img}", alt: "", loading: "lazy" }
                            div { class: "flex-1 min-w-0",
                                p { class: "text-sm font-medium line-clamp-2", "{r.title}" }
                                if !author.is_empty() {
                                    p { class: "text-xs text-base-content/60 truncate", "{author}" }
                                }
                                if !details.is_empty() {
                                    p { class: "text-xs text-base-content/40 truncate", "{details}" }
                                }
                            }
                            if subscribed {
                                span { class: "badge badge-success badge-sm gap-1 shrink-0",
                                    i { class: "material-icons text-xs", "check" }
                                    "Subscribed"
                                }
                            } else {
                                button {
                                    class: "btn btn-primary btn-xs shrink-0",
                                    disabled: busy,
                                    onclick: move |_| on_subscribe.call(feed_url.clone()),
                                    i { class: "material-icons text-sm", "add" }
                                    "Subscribe"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats() {
        assert_eq!(fmt_duration(3723), "1h 02m");
        assert_eq!(fmt_duration(1800), "30m");
        assert_eq!(fmt_duration(58), "0:58");
    }

    #[test]
    fn meta_line_shows_remaining_time_only_while_in_progress() {
        let mut ep = Episode {
            duration_secs: Some(3600),
            position_secs: 600,
            ..Default::default()
        };
        assert_eq!(episode_meta(&ep), "1h 00m • 50m left");
        ep.played = true;
        assert_eq!(episode_meta(&ep), "1h 00m");
        ep.played = false;
        ep.duration_secs = None;
        assert_eq!(episode_meta(&ep), "at 10m");
    }

    #[test]
    fn feed_url_detection() {
        assert!(is_feed_url(" https://example.com/feed.xml"));
        assert!(!is_feed_url("planet money"));
    }
}
