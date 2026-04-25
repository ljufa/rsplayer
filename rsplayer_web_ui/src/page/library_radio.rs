use api_models::common::{MetadataCommand, QueueCommand, UserCommand};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

const RADIO_BROWSER_URL: &str = "https://de2.api.radio-browser.info/json/";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Country {
    name: String,
    iso_3166_1: String,
    stationcount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Language {
    name: String,
    stationcount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Tag {
    name: String,
    stationcount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Station {
    stationuuid: String,
    name: String,
    url: String,
    favicon: String,
    tags: String,
    language: String,
    votes: usize,
    codec: String,
    bitrate: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum FilterType {
    Favorites,
    Country,
    Language,
    Tag,
    Search,
}

#[derive(Debug, Clone)]
enum BrowseItem {
    Country(Country),
    Language(Language),
    Tag(Tag),
    Station(Station),
}

#[component]
pub fn LibraryRadioPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let mut filter = use_signal(|| FilterType::Favorites);
    let mut loading = use_signal(|| *filter.read() == FilterType::Favorites);
    let mut browse_items: Signal<Vec<BrowseItem>> = use_signal(Vec::new);
    let mut stations: Signal<Vec<Station>> = use_signal(Vec::new);
    let mut showing_stations = use_signal(|| false);
    let mut search = use_signal(String::new);

    // Query favorites on mount
    use_effect(move || {
        if *filter.read() == FilterType::Favorites {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations));
        }
    });

    // When favorite UUIDs arrive, fetch station details
    use_effect(move || {
        let uuids = state.favorite_radio_stations.read().clone();
        if !uuids.is_empty() && *filter.read() == FilterType::Favorites {
            spawn(async move {
                let fetched = fetch_stations_by_uuid(uuids).await;
                *stations.write() = fetched;
                *loading.write() = false;
                *showing_stations.write() = true;
            });
        } else if uuids.is_empty() && *filter.read() == FilterType::Favorites {
            *loading.write() = false;
        }
    });

    let mut change_filter = move |new_filter: FilterType| {
        *filter.write() = new_filter.clone();
        *loading.write() = true;
        *showing_stations.write() = false;
        *browse_items.write() = Vec::new();
        *stations.write() = Vec::new();
        match new_filter {
            FilterType::Favorites => {
                ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations));
            }
            FilterType::Country => {
                spawn(async move {
                    let url = format!("{RADIO_BROWSER_URL}countries?limit=200&hidebroken=true");
                    if let Ok(resp) = Request::get(&url).send().await {
                        if let Ok(list) = resp.json::<Vec<Country>>().await {
                            *browse_items.write() = list.into_iter().map(BrowseItem::Country).collect();
                        }
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Language => {
                spawn(async move {
                    let url = format!("{RADIO_BROWSER_URL}languages?limit=500");
                    if let Ok(resp) = Request::get(&url).send().await {
                        if let Ok(list) = resp.json::<Vec<Language>>().await {
                            *browse_items.write() = list.into_iter().map(BrowseItem::Language).collect();
                        }
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Tag => {
                spawn(async move {
                    let url =
                        format!("{RADIO_BROWSER_URL}tags?limit=500&order=stationcount&reverse=true&hidebroken=true");
                    if let Ok(resp) = Request::get(&url).send().await {
                        if let Ok(list) = resp.json::<Vec<Tag>>().await {
                            *browse_items.write() = list.into_iter().map(BrowseItem::Tag).collect();
                        }
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Search => {
                *loading.write() = false;
            }
        }
    };

    rsx! {
        div { class: "library-page",
            // ── Filter tabs ────────────────────────────────────────────────
            div { class: "flex gap-2 px-3 py-2 border-b border-base-300 overflow-x-auto",
                for (label, ft) in [
                    ("Favorites", FilterType::Favorites),
                    ("Search", FilterType::Search),
                    ("Countries", FilterType::Country),
                    ("Languages", FilterType::Language),
                    ("Tags", FilterType::Tag),
                ] {
                    button {
                        key: "{label}",
                        class: if *filter.read() == ft {
                            "btn btn-sm btn-primary"
                        } else {
                            "btn btn-sm btn-ghost"
                        },
                        onclick: {
                            let ft = ft.clone();
                            move |_| change_filter(ft.clone())
                        },
                        "{label}"
                    }
                }
            }

            // ── Search input (only for Search tab) ────────────────────────────
            if *filter.read() == FilterType::Search {
                div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300",
                    input {
                        class: "input input-sm input-bordered flex-1",
                        r#type: "text",
                        placeholder: "Search stations by name…",
                        value: "{search}",
                        autofocus: true,
                        oninput: move |e| search.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let term = search();
                                spawn(async move {
                                    *loading.write() = true;
                                    let fetched = search_stations_by_name(&term).await;
                                    *stations.write() = fetched;
                                    *showing_stations.write() = true;
                                    *loading.write() = false;
                                });
                            }
                        },
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Search",
                        onclick: move |_| {
                            let term = search();
                            spawn(async move {
                                *loading.write() = true;
                                let fetched = search_stations_by_name(&term).await;
                                *stations.write() = fetched;
                                *showing_stations.write() = true;
                                *loading.write() = false;
                            });
                        },
                        i { class: "material-icons text-base", "search" }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Clear search",
                        onclick: move |_| {
                            search.set(String::new());
                            *stations.write() = Vec::new();
                        },
                        i { class: "material-icons text-base", "backspace" }
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Load all to queue",
                        onclick: move |_| {
                            for st in stations.read().iter() {
                                ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongToQueue(st.url.clone())));
                            }
                        },
                        i { class: "material-icons text-base", "playlist_play" }
                    }
                }
            }

            // ── Content ────────────────────────────────────────────────────
            if loading() && *filter.read() != FilterType::Favorites {
                div { class: "flex flex-col gap-1 p-3",
                    {(0..8).map(|_| rsx! {
                        div { class: "flex items-center gap-2 py-1.5 px-2",
                            div { class: "skeleton w-8 h-8 rounded-full" }
                            div { class: "flex-1 flex flex-col gap-1",
                                div { class: "skeleton h-4 w-3/4 rounded" }
                                div { class: "skeleton h-3 w-1/2 rounded" }
                            }
                        }
                    })}
                }
            } else if *filter.read() == FilterType::Favorites && loading() {
                div { class: "flex flex-col gap-1 p-3",
                    {(0..4).map(|_| rsx! {
                        div { class: "flex items-center gap-2 py-1.5 px-2",
                            div { class: "skeleton w-8 h-8 rounded-full" }
                            div { class: "flex-1 flex flex-col gap-1",
                                div { class: "skeleton h-4 w-3/4 rounded" }
                                div { class: "skeleton h-3 w-1/2 rounded" }
                            }
                        }
                    })}
                }
            } else if showing_stations() {
                // Station list
                if stations.read().is_empty() {
                    div { class: "flex flex-col items-center py-16 gap-3 text-base-content/40",
                        i { class: "material-icons text-5xl", "radio" }
                        p {
                            if *filter.read() == FilterType::Favorites {
                                "No favorite stations yet."
                            } else if *filter.read() == FilterType::Search {
                                "No stations found. Try a different search term."
                            } else {
                                "No stations found."
                            }
                        }
                    }
                } else {
                    div { class: "overflow-y-auto",
                        {
                            let is_favorites = *filter.read() == FilterType::Favorites;
                            stations.read().iter()
                                .cloned()
                                .map(move |st| {
                                    let url = st.url.clone();
                                    let url2 = st.url.clone();
                                    let uuid = st.stationuuid.clone();
                                    let key = st.stationuuid.clone();
                                    rsx! {
                                        div {
                                            key: "{key}",
                                            class: "flex items-center gap-3 px-3 py-2 hover:bg-base-200 group",
                                            if !st.favicon.is_empty() {
                                                img {
                                                    class: "w-8 h-8 rounded-full object-cover",
                                                    src: "{st.favicon}",
                                                }
                                            } else {
                                                span { class: "w-8 h-8 flex items-center justify-center rounded-full bg-base-300",
                                                    i { class: "material-icons text-sm", "radio" }
                                                }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-sm font-medium truncate", "{st.name}" }
                                                p { class: "text-xs text-base-content/50 truncate",
                                                    "{st.codec} {st.bitrate}kbps • {st.tags}"
                                                }
                                            }
                                            div { class: "flex sm:hidden sm:group-hover:flex items-center gap-1",
                                                button {
                                                    class: "btn btn-ghost btn-xs",
                                                    title: "Add to queue",
                                                    onclick: move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongToQueue(url.clone()))),
                                                    i { class: "material-icons text-sm", "playlist_add" }
                                                }
                                                button {
                                                    class: "btn btn-ghost btn-xs",
                                                    title: "Play now",
                                                    onclick: move |_| ws_send(&ws, &UserCommand::Queue(QueueCommand::AddSongAndPlay(url2.clone()))),
                                                    i { class: "material-icons text-sm", "play_arrow" }
                                                }
                                                if is_favorites {
                                                    button {
                                                        class: "btn btn-ghost btn-xs text-error",
                                                        title: "Remove from favorites",
                                                        onclick: move |_| {
                                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::DislikeMediaItem(format!("radio_uuid_{}", uuid.clone()))));
                                                            // Re-query to refresh list
                                                            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations));
                                                        },
                                                        i { class: "material-icons text-sm", "favorite" }
                                                    }
                                                } else {
                                                    button {
                                                        class: "btn btn-ghost btn-xs",
                                                        title: "Add to favorites",
                                                        onclick: move |_| ws_send(&ws, &UserCommand::Metadata(MetadataCommand::LikeMediaItem(format!("radio_uuid_{}", uuid)))),
                                                        i { class: "material-icons text-sm", "favorite_border" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                })
                        }
                    }
                }
            } else {
                // Browse items list (country/language/tag)
                div { class: "overflow-y-auto",
                    {browse_items.read().iter().cloned().map(|item| {
                        let (label, _count) = match &item {
                            BrowseItem::Country(c) => (format!("{} ({})", c.name, c.stationcount), c.stationcount),
                            BrowseItem::Language(l) => (format!("{} ({})", l.name, l.stationcount), l.stationcount),
                            BrowseItem::Tag(t) => (format!("{} ({})", t.name, t.stationcount), t.stationcount),
                            BrowseItem::Station(s) => (s.name.clone(), 0),
                        };
                        let key = match &item {
                            BrowseItem::Country(c) => format!("country-{}", c.iso_3166_1),
                            BrowseItem::Language(l) => format!("language-{}", l.name),
                            BrowseItem::Tag(t) => format!("tag-{}", t.name),
                            BrowseItem::Station(s) => format!("station-{}", s.stationuuid),
                        };
                        rsx! {
                            div {
                                key: "{key}",
                                class: "flex items-center gap-2 px-3 py-2 hover:bg-base-200 group cursor-pointer",
                                onclick: move |_| {
                                    *loading.write() = true;
                                    *showing_stations.write() = false;
                                    let item = item.clone();
                                    spawn(async move {
                                        let fetched = match &item {
                                            BrowseItem::Country(c) => {
                                                fetch_stations("bycountrycodeexact", &c.iso_3166_1).await
                                            }
                                            BrowseItem::Language(l) => {
                                                fetch_stations("bylanguageexact", &l.name).await
                                            }
                                            BrowseItem::Tag(t) => {
                                                fetch_stations("bytagexact", &t.name).await
                                            }
                                            _ => vec![],
                                        };
                                        *stations.write() = fetched;
                                        *loading.write() = false;
                                        *showing_stations.write() = true;
                                    });
                                },
                                i { class: "material-icons text-sm text-base-content/50", "chevron_right" }
                                span { class: "flex-1 text-sm truncate", "{label}" }
                            }
                        }
                    })}
                }
            }
        }
    }
}

async fn fetch_stations_by_uuid(uuids: Vec<String>) -> Vec<Station> {
    let url = format!("{RADIO_BROWSER_URL}stations/byuuid?uuids={}", uuids.join(","));
    let Ok(resp) = Request::get(&url).send().await else {
        return vec![];
    };
    resp.json::<Vec<Station>>().await.unwrap_or_default()
}

async fn fetch_stations(by: &str, value: &str) -> Vec<Station> {
    let url = format!(
        "{RADIO_BROWSER_URL}stations/{by}/{}?limit=300&hidebroken=true&order=votes&reverse=true",
        value
    );
    let Ok(resp) = Request::get(&url).send().await else {
        return vec![];
    };
    resp.json::<Vec<Station>>().await.unwrap_or_default()
}

async fn search_stations_by_name(name: &str) -> Vec<Station> {
    let url = format!(
        "{RADIO_BROWSER_URL}stations/search?name={}&limit=300&hidebroken=true",
        name
    );
    let Ok(resp) = Request::get(&url).send().await else {
        return vec![];
    };
    resp.json::<Vec<Station>>().await.unwrap_or_default()
}
