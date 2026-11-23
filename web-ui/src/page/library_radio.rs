//! Library → Radio: radio-browser discovery (countries/languages/tags/search),
//! favourites (radio-browser uuids whose details the server also keeps), and
//! stations the user typed in by hand, which the server stores so they
//! survive queue clears and restarts. Stations play directly, without being
//! added to the queue.

use api_models::common::{MetadataCommand, PlayerCommand, QueueCommand, UserCommand};
use api_models::playback_source::PlaybackSource;
use api_models::radio::RadioStation;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use web_sys::WebSocket;

use crate::{hooks::ws_send, state::AppState};

const RADIO_BROWSER_URL: &str = "https://de2.api.radio-browser.info/json/";

/// radio-browser requests are given up after this long, so an unreachable
/// API shows an error instead of an endless skeleton.
const RADIO_BROWSER_TIMEOUT_MS: u32 = 15_000;

const RADIO_BROWSER_UNREACHABLE: &str =
    "Couldn't reach radio-browser.info, the station directory. Check the internet connection or try again later.";

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

impl Station {
    /// The server-side representation, used to play or favorite the station.
    fn to_record(&self) -> RadioStation {
        RadioStation {
            name: self.name.clone(),
            url: self.url.clone(),
            image_url: Some(self.favicon.clone()).filter(|f| !f.is_empty()),
            radio_browser_uuid: Some(self.stationuuid.clone()),
            ..Default::default()
        }
    }

    /// A favorite built from the details the server keeps, for when
    /// radio-browser hasn't answered (yet). `None` for a hand-added station.
    fn from_record(record: &RadioStation) -> Option<Self> {
        Some(Self {
            stationuuid: record.radio_browser_uuid.clone()?,
            name: record.name.clone(),
            url: record.url.clone(),
            favicon: record.image_url.clone().unwrap_or_default(),
            tags: String::new(),
            language: String::new(),
            votes: 0,
            codec: String::new(),
            bitrate: 0,
        })
    }

    /// Second line of a station row: format details from radio-browser, or
    /// the stream URL when only the server's copy is known.
    fn subtitle(&self) -> String {
        if self.codec.is_empty() && self.tags.is_empty() {
            self.url.clone()
        } else {
            format!("{} {}kbps • {}", self.codec, self.bitrate, self.tags)
        }
    }
}

/// True when `url` is the station the player is currently tuned to.
fn is_playing_station(state: &AppState, url: &str) -> bool {
    matches!(&*state.playback_source.read(), PlaybackSource::Radio(s) if s.url == url)
}

fn play_station(ws: &Signal<Option<WebSocket>>, station: RadioStation) {
    ws_send(ws, &UserCommand::Player(PlayerCommand::PlayRadioStation(station)));
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
}

#[component]
pub fn LibraryRadioPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let custom_stations = state.custom_radio_stations;
    let mut filter = use_signal(|| FilterType::Favorites);
    let mut loading = use_signal(|| *filter.read() == FilterType::Favorites);
    let mut browse_items: Signal<Vec<BrowseItem>> = use_signal(Vec::new);
    let mut stations: Signal<Vec<Station>> = use_signal(Vec::new);
    let mut showing_stations = use_signal(|| false);
    let mut search = use_signal(String::new);
    // `Some` while the add/edit dialog is open; an empty id means a new station.
    let mut editing: Signal<Option<RadioStation>> = use_signal(|| None);
    // radio-browser details of favorites (codec, bitrate, tags), filled in
    // when the API answers; the list itself doesn't depend on it.
    let mut favorite_details: Signal<Vec<Station>> = use_signal(Vec::new);
    // Set when the last radio-browser request failed or timed out.
    let mut fetch_error = use_signal(|| false);

    // Query favorites on mount
    use_effect(move || {
        if *filter.read() == FilterType::Favorites {
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations));
            ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryCustomRadioStations));
        }
    });

    // Favorites list: from the details the server keeps, so it shows (and
    // plays) even when radio-browser.info is slow or unreachable. radio-browser
    // details replace them once they arrive.
    use_effect(move || {
        let uuids = state.favorite_radio_stations.read().clone();
        let records = state.favorite_radio_station_records.read().clone();
        let details = favorite_details.read().clone();
        if *filter.read() != FilterType::Favorites {
            return;
        }
        let list: Vec<Station> = uuids
            .iter()
            .filter_map(|uuid| {
                details.iter().find(|d| &d.stationuuid == uuid).cloned().or_else(|| {
                    records
                        .iter()
                        .find(|r| r.radio_browser_uuid.as_ref() == Some(uuid))
                        .and_then(Station::from_record)
                })
            })
            .collect();
        *stations.write() = list;
        *loading.write() = false;
        *showing_stations.write() = true;
    });

    // When favorite UUIDs arrive, ask radio-browser for their current details.
    use_effect(move || {
        let uuids = state.favorite_radio_stations.read().clone();
        if uuids.is_empty() || *filter.read() != FilterType::Favorites {
            return;
        }
        spawn(async move {
            let Ok(fetched) = fetch_stations_by_uuid(&uuids).await else {
                // Favorites still show from the server's copy; only say
                // something when some of them can't be shown at all.
                let known = state.favorite_radio_station_records.peek();
                let missing = uuids
                    .iter()
                    .any(|u| !known.iter().any(|k| k.radio_browser_uuid.as_ref() == Some(u)));
                if missing && *filter.peek() == FilterType::Favorites {
                    fetch_error.set(true);
                }
                return;
            };
            // Favorites liked before the server kept station details:
            // send them so Next/Prev can switch between all favorites.
            let known = state.favorite_radio_station_records.peek().clone();
            for st in fetched
                .iter()
                .filter(|st| !known.iter().any(|k| k.radio_browser_uuid.as_deref() == Some(st.stationuuid.as_str())))
            {
                ws_send(&ws, &UserCommand::Metadata(MetadataCommand::LikeRadioStation(st.to_record())));
            }
            favorite_details.set(fetched);
        });
    });

    let mut change_filter = move |new_filter: FilterType| {
        *filter.write() = new_filter;
        *loading.write() = true;
        *showing_stations.write() = false;
        *browse_items.write() = Vec::new();
        *stations.write() = Vec::new();
        fetch_error.set(false);
        match new_filter {
            FilterType::Favorites => {
                ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations));
                ws_send(&ws, &UserCommand::Metadata(MetadataCommand::QueryCustomRadioStations));
            }
            FilterType::Country => {
                spawn(async move {
                    let url = format!("{RADIO_BROWSER_URL}countries?limit=200&hidebroken=true");
                    let result = get_json::<Vec<Country>>(&url).await;
                    if *filter.peek() != new_filter {
                        return;
                    }
                    match result {
                        Ok(list) => *browse_items.write() = list.into_iter().map(BrowseItem::Country).collect(),
                        Err(()) => fetch_error.set(true),
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Language => {
                spawn(async move {
                    let url = format!("{RADIO_BROWSER_URL}languages?limit=500");
                    let result = get_json::<Vec<Language>>(&url).await;
                    if *filter.peek() != new_filter {
                        return;
                    }
                    match result {
                        Ok(list) => *browse_items.write() = list.into_iter().map(BrowseItem::Language).collect(),
                        Err(()) => fetch_error.set(true),
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Tag => {
                spawn(async move {
                    let url = format!("{RADIO_BROWSER_URL}tags?limit=500&order=stationcount&reverse=true&hidebroken=true");
                    let result = get_json::<Vec<Tag>>(&url).await;
                    if *filter.peek() != new_filter {
                        return;
                    }
                    match result {
                        Ok(list) => *browse_items.write() = list.into_iter().map(BrowseItem::Tag).collect(),
                        Err(()) => fetch_error.set(true),
                    }
                    *loading.write() = false;
                });
            }
            FilterType::Search => {
                *loading.write() = false;
            }
        }
    };

    let mut run_search = move || {
        let term = search();
        fetch_error.set(false);
        spawn(async move {
            *loading.write() = true;
            let result = search_stations_by_name(&term).await;
            if *filter.peek() != FilterType::Search {
                return;
            }
            fetch_error.set(result.is_err());
            *stations.write() = result.unwrap_or_default();
            *showing_stations.write() = true;
            *loading.write() = false;
        });
    };

    rsx! {
        div { class: "library-page",
            // ── Filter tabs ────────────────────────────────────────────────
            div { class: "flex gap-2 px-3 py-2 border-b border-base-300 overflow-x-auto",
                for (label , ft) in [
                    ("Favorites", FilterType::Favorites),
                    ("Search", FilterType::Search),
                    ("Countries", FilterType::Country),
                    ("Languages", FilterType::Language),
                    ("Tags", FilterType::Tag),
                ]
                {
                    button {
                        key: "{label}",
                        class: if *filter.read() == ft { "btn btn-sm btn-primary" } else { "btn btn-sm btn-ghost" },
                        onclick: move |_| change_filter(ft),
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
                                run_search();
                            }
                        },
                    }
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "Search",
                        onclick: move |_| run_search(),
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
                                ws_send(
                                    &ws,
                                    &UserCommand::Queue(QueueCommand::AddSongToQueue(st.url.clone())),
                                );
                            }
                        },
                        i { class: "material-icons text-base", "playlist_play" }
                    }
                }
            }

            // ── Add station (only for Favorites tab) ─────────────────────────
            if *filter.read() == FilterType::Favorites {
                div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300",
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| editing.set(Some(RadioStation::default())),
                        i { class: "material-icons text-base", "add" }
                        "Add station"
                    }
                    span { class: "text-xs text-base-content/50 truncate",
                        "Keep any stream URL here permanently"
                    }
                }
            }

            // ── Content ────────────────────────────────────────────────────
            if fetch_error() && !loading() {
                div { class: "alert alert-warning m-3 py-2 text-sm",
                    i { class: "material-icons text-base", "cloud_off" }
                    span { "{RADIO_BROWSER_UNREACHABLE}" }
                }
            }
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
                if stations.read().is_empty() && (*filter.read() != FilterType::Favorites || custom_stations.read().is_empty()) {
                    div { class: "flex flex-col items-center py-16 gap-3 text-base-content/40",
                        i { class: "material-icons text-5xl", "radio" }
                        if *filter.read() == FilterType::Favorites {
                            p { class: "text-center", "No favourite stations yet." }
                            p { class: "text-sm text-center",
                                "Add a stream URL with the button above, or browse by Country, Language or Tags."
                            }
                        } else if *filter.read() == FilterType::Search {
                            p { "No stations found. Try a different search term." }
                        } else {
                            p { "No stations found." }
                        }
                    }
                } else {
                    div { class: "overflow-y-auto",
                        // Hand-added stations come first, above the radio-browser favourites.
                        if *filter.read() == FilterType::Favorites {
                            {
                                custom_stations
                                    .read()
                                    .iter()
                                    .cloned()
                                    .map(|st| {
                                        let key = st.id.clone();
                                        rsx! {
                                            CustomStationRow {
                                                key: "{key}",
                                                station: st,
                                                on_edit: move |s: RadioStation| editing.set(Some(s)),
                                            }
                                        }
                                    })
                            }
                        }
                        {
                            let is_favorites = *filter.read() == FilterType::Favorites;
                            stations
                                .read()
                                .iter()
                                .map(move |st| {
                                    let url = st.url.clone();
                                    let uuid = st.stationuuid.clone();
                                    let key = st.stationuuid.clone();
                                    let record = st.to_record();
                                    let record_row = record.clone();
                                    let record_like = record.clone();
                                    let playing = is_playing_station(&state, &st.url);
                                    let row_bg = if playing { "bg-primary/10" } else { "hover:bg-base-200" };
                                    rsx! {
                                        div {
                                            key: "{key}",
                                            class: "flex items-center gap-3 px-3 py-2 group {row_bg}",
                                            div {
                                                class: "flex flex-1 min-w-0 items-center gap-3 cursor-pointer",
                                                title: "Play",
                                                onclick: move |_| play_station(&ws, record_row.clone()),
                                                if !st.favicon.is_empty() {
                                                    img { class: "w-8 h-8 rounded-full object-cover", src: "{st.favicon}" }
                                                } else {
                                                    span { class: "w-8 h-8 flex items-center justify-center rounded-full bg-base-300",
                                                        i { class: "material-icons text-sm", "radio" }
                                                    }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    p { class: if playing { "text-sm font-medium truncate text-primary" } else { "text-sm font-medium truncate" },
                                                        "{st.name}"
                                                    }
                                                    p { class: "text-xs text-base-content/50 truncate",
                                                        "{st.subtitle()}"
                                                    }
                                                }
                                            }
                                            if playing {
                                                i { class: "material-icons text-sm text-primary", title: "Playing", "graphic_eq" }
                                            }
                                            div { class: "flex sm:hidden sm:group-hover:flex items-center gap-1",
                                                button {
                                                    class: "btn btn-ghost btn-xs",
                                                    title: "Play now",
                                                    onclick: move |_| play_station(&ws, record.clone()),
                                                    i { class: "material-icons text-sm", "play_arrow" }
                                                }
                                                button {
                                                    class: "btn btn-ghost btn-xs opacity-60",
                                                    title: "Add to queue",
                                                    onclick: move |_| ws_send(
                                                        &ws,
                                                        &UserCommand::Queue(QueueCommand::AddSongToQueue(url.clone())),
                                                    ),
                                                    i { class: "material-icons text-sm", "playlist_add" }
                                                }
                                                if is_favorites {
                                                    button {
                                                        class: "btn btn-ghost btn-xs text-error",
                                                        title: "Remove from favorites",
                                                        onclick: move |_| {
                                                            ws_send(
                                                                // Re-query to refresh list
                                                                &ws,
                                                                &UserCommand::Metadata(
                                                                    MetadataCommand::DislikeMediaItem(format!("radio_uuid_{}", uuid.clone())),
                                                                ),
                                                            );
                                                            ws_send(
                                                                &ws,
                                                                &UserCommand::Metadata(MetadataCommand::QueryFavoriteRadioStations),
                                                            );
                                                        },
                                                        i { class: "material-icons text-sm", "favorite" }
                                                    }
                                                } else {
                                                    button {
                                                        class: "btn btn-ghost btn-xs",
                                                        title: "Add to favorites",
                                                        onclick: move |_| ws_send(
                                                            &ws,
                                                            &UserCommand::Metadata(MetadataCommand::LikeRadioStation(record_like.clone())),
                                                        ),
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
                    {
                        browse_items
                            .read()
                            .iter()
                            .cloned()
                            .map(|item| {
                                let (label, _count) = match &item {
                                    BrowseItem::Country(c) => {
                                        (format!("{} ({})", c.name, c.stationcount), c.stationcount)
                                    }
                                    BrowseItem::Language(l) => {
                                        (format!("{} ({})", l.name, l.stationcount), l.stationcount)
                                    }
                                    BrowseItem::Tag(t) => {
                                        (format!("{} ({})", t.name, t.stationcount), t.stationcount)
                                    }
                                };
                                let key = match &item {
                                    BrowseItem::Country(c) => format!("country-{}", c.iso_3166_1),
                                    BrowseItem::Language(l) => format!("language-{}", l.name),
                                    BrowseItem::Tag(t) => format!("tag-{}", t.name),
                                };
                                rsx! {
                                    div {
                                        key: "{key}",
                                        class: "flex items-center gap-2 px-3 py-2 hover:bg-base-200 group cursor-pointer",
                                        onclick: move |_| {
                                            *loading.write() = true;
                                            *showing_stations.write() = false;
                                            let item = item.clone();
                                            let current = *filter.peek();
                                            spawn(async move {
                                                let result = match &item {
                                                    BrowseItem::Country(c) => {
                                                        fetch_stations("bycountrycodeexact", &c.iso_3166_1).await
                                                    }
                                                    BrowseItem::Language(l) => {
                                                        fetch_stations("bylanguageexact", &l.name).await
                                                    }
                                                    BrowseItem::Tag(t) => fetch_stations("bytagexact", &t.name).await,
                                                };
                                                if *filter.peek() != current {
                                                    return;
                                                }
                                                fetch_error.set(result.is_err());
                                                *stations.write() = result.unwrap_or_default();
                                                *loading.write() = false;
                                                *showing_stations.write() = true;
                                            });
                                        },
                                        i { class: "material-icons text-sm text-base-content/50", "chevron_right" }
                                        span { class: "flex-1 text-sm truncate", "{label}" }
                                    }
                                }
                            })
                    }
                }
            }

            if let Some(station) = editing() {
                CustomStationDialog { station, on_close: move |()| editing.set(None) }
            }
        }
    }
}

/// One hand-added station: play/queue it, edit it, or drop it.
#[component]
fn CustomStationRow(station: RadioStation, on_edit: EventHandler<RadioStation>) -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let mut confirm_delete = use_signal(|| false);

    let url_queue = station.url.clone();
    let station_play = station.clone();
    let station_row = station.clone();
    let playing = is_playing_station(&state, &station.url);
    let row_bg = if playing { "bg-primary/10" } else { "hover:bg-base-200" };
    let station_edit = station.clone();
    let id = station.id.clone();
    let logo = station.image_url.clone().unwrap_or_default();

    rsx! {
        div { class: "flex items-center gap-3 px-3 py-2 group {row_bg}",
            div {
                class: "flex flex-1 min-w-0 items-center gap-3 cursor-pointer",
                title: "Play",
                onclick: move |_| play_station(&ws, station_row.clone()),
                if logo.is_empty() {
                    span { class: "w-8 h-8 flex items-center justify-center rounded-full bg-base-300",
                        i { class: "material-icons text-sm", "radio" }
                    }
                } else {
                    img { class: "w-8 h-8 rounded-full object-cover", src: "{logo}" }
                }
                div { class: "flex-1 min-w-0",
                    p { class: if playing { "text-sm font-medium truncate text-primary" } else { "text-sm font-medium truncate" },
                        "{station.name}"
                    }
                    p { class: "text-xs text-base-content/50 truncate", "{station.url}" }
                }
            }
            if playing {
                i { class: "material-icons text-sm text-primary", title: "Playing", "graphic_eq" }
            }
            if confirm_delete() {
                div { class: "flex items-center gap-1",
                    button {
                        class: "btn btn-xs btn-error",
                        onclick: move |_| {
                            ws_send(
                                &ws,
                                &UserCommand::Metadata(MetadataCommand::DeleteCustomRadioStation(id.clone())),
                            );
                            confirm_delete.set(false);
                        },
                        "Remove"
                    }
                    button {
                        class: "btn btn-xs btn-ghost",
                        onclick: move |_| confirm_delete.set(false),
                        "Cancel"
                    }
                }
            } else {
                div { class: "flex sm:hidden sm:group-hover:flex items-center gap-1",
                    button {
                        class: "btn btn-ghost btn-xs",
                        title: "Play now",
                        onclick: move |_| play_station(&ws, station_play.clone()),
                        i { class: "material-icons text-sm", "play_arrow" }
                    }
                    button {
                        class: "btn btn-ghost btn-xs opacity-60",
                        title: "Add to queue",
                        onclick: move |_| ws_send(
                            &ws,
                            &UserCommand::Queue(QueueCommand::AddSongToQueue(url_queue.clone())),
                        ),
                        i { class: "material-icons text-sm", "playlist_add" }
                    }
                    button {
                        class: "btn btn-ghost btn-xs",
                        title: "Edit station",
                        onclick: move |_| on_edit.call(station_edit.clone()),
                        i { class: "material-icons text-sm", "edit" }
                    }
                    button {
                        class: "btn btn-ghost btn-xs text-error",
                        title: "Remove station",
                        onclick: move |_| confirm_delete.set(true),
                        i { class: "material-icons text-sm", "delete" }
                    }
                }
            }
        }
    }
}

/// Add/edit form for a hand-added station. An empty `station.id` means a new one.
#[component]
fn CustomStationDialog(station: RadioStation, on_close: EventHandler) -> Element {
    let ws = use_context::<Signal<Option<WebSocket>>>();
    let is_new = station.id.is_empty();

    // Signals only: the closure below must stay `Copy` to be used by several handlers.
    let id = use_signal(|| station.id.clone());
    let added_at = use_signal(|| station.added_at);
    let mut name = use_signal(|| station.name.clone());
    let mut url = use_signal(|| station.url.clone());
    let mut image_url = use_signal(|| station.image_url.clone().unwrap_or_default());
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let mut save = move || {
        let candidate = RadioStation {
            id: id(),
            name: name(),
            url: url(),
            image_url: Some(image_url()),
            added_at: added_at(),
            radio_browser_uuid: None,
        };
        match candidate.validated() {
            Ok(valid) => {
                ws_send(&ws, &UserCommand::Metadata(MetadataCommand::SaveCustomRadioStation(valid)));
                on_close.call(());
            }
            Err(msg) => error.set(Some(msg)),
        }
    };

    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-box max-w-md",
                h3 { class: "font-bold text-lg mb-3",
                    if is_new { "Add radio station" } else { "Edit radio station" }
                }
                div { class: "flex flex-col gap-3",
                    label { class: "form-control w-full",
                        div { class: "label", span { class: "label-text text-sm", "Station name" } }
                        input {
                            class: "input input-bordered input-sm w-full",
                            r#type: "text",
                            placeholder: "My favourite station",
                            autofocus: true,
                            value: "{name}",
                            oninput: move |e| { name.set(e.value()); error.set(None); },
                            onkeydown: move |e| if e.key() == Key::Enter { save() },
                        }
                    }
                    label { class: "form-control w-full",
                        div { class: "label", span { class: "label-text text-sm", "Stream URL" } }
                        input {
                            class: "input input-bordered input-sm w-full",
                            r#type: "url",
                            placeholder: "https://example.com/stream.mp3",
                            value: "{url}",
                            oninput: move |e| { url.set(e.value()); error.set(None); },
                            onkeydown: move |e| if e.key() == Key::Enter { save() },
                        }
                    }
                    label { class: "form-control w-full",
                        div { class: "label", span { class: "label-text text-sm", "Logo URL (optional)" } }
                        input {
                            class: "input input-bordered input-sm w-full",
                            r#type: "url",
                            placeholder: "https://example.com/logo.png",
                            value: "{image_url}",
                            oninput: move |e| { image_url.set(e.value()); error.set(None); },
                            onkeydown: move |e| if e.key() == Key::Enter { save() },
                        }
                    }
                    if let Some(msg) = error() {
                        div { class: "alert alert-error py-2 text-sm", "{msg}" }
                    }
                }
                div { class: "modal-action",
                    button { class: "btn btn-sm btn-ghost", onclick: move |_| on_close.call(()), "Cancel" }
                    button { class: "btn btn-sm btn-primary", onclick: move |_| save(), "Save" }
                }
            }
            div { class: "modal-backdrop", onclick: move |_| on_close.call(()) }
        }
    }
}

/// GETs a radio-browser endpoint, giving up after [`RADIO_BROWSER_TIMEOUT_MS`].
async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, ()> {
    let controller = web_sys::AbortController::new().ok();
    if let Some(c) = controller.clone() {
        wasm_bindgen_futures::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(RADIO_BROWSER_TIMEOUT_MS).await;
            c.abort();
        });
    }
    let signal = controller.as_ref().map(web_sys::AbortController::signal);
    let resp = Request::get(url).abort_signal(signal.as_ref()).send().await.map_err(|_| ())?;
    if !resp.ok() {
        return Err(());
    }
    resp.json::<T>().await.map_err(|_| ())
}

fn encode(value: &str) -> String {
    String::from(js_sys::encode_uri_component(value))
}

async fn fetch_stations_by_uuid(uuids: &[String]) -> Result<Vec<Station>, ()> {
    get_json(&format!("{RADIO_BROWSER_URL}stations/byuuid?uuids={}", uuids.join(","))).await
}

async fn fetch_stations(by: &str, value: &str) -> Result<Vec<Station>, ()> {
    let value = encode(value);
    get_json(&format!(
        "{RADIO_BROWSER_URL}stations/{by}/{value}?limit=300&hidebroken=true&order=votes&reverse=true"
    ))
    .await
}

async fn search_stations_by_name(name: &str) -> Result<Vec<Station>, ()> {
    let name = encode(name);
    get_json(&format!("{RADIO_BROWSER_URL}stations/search?name={name}&limit=300&hidebroken=true")).await
}
