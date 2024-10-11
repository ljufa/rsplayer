use std::vec;

use api_models::common::{MetadataCommand, QueueCommand, UserCommand};
use api_models::state::StateChangeEvent;
use gloo_net::http::Request;
use indextree::{Arena, NodeId};
use seed::prelude::web_sys::KeyboardEvent;
use seed::{a, attrs, div, empty, i, img, input, label, li, p, prelude::*, section, span, style, ul, C, IF};
use serde::{Deserialize, Serialize};

use crate::page::music_library_radio::Msg::{
    AddItemToQueue, ChangeCategory, CollapseNodeClick, CountriesFetched, ExpandNodeClick, LanguagesFetched,
    LoadItemToQueue, StationsFetched, TagsFetched,
};
use crate::view_spinner_modal;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Country {
    name: String,
    iso_3166_1: String,
    stationcount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Station {
    stationuuid: String,
    name: String,
    url: String,
    favicon: String,
    tags: String,
    language: String,
    state: String,
    votes: usize,
    codec: String,
    bitrate: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Language {
    name: String,
    stationcount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag {
    name: String,
    stationcount: usize,
}

#[derive(Debug, Clone)]
enum TreeNode {
    Root,
    Country(Country),
    Language(Language),
    Tag(Tag),
    Station(Station),
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    SendUserCommand(UserCommand),
    FavoriteRadioStation(NodeId),
    UnfavoriteRadioStation(NodeId),
    AddItemToQueue(NodeId),
    LoadItemToQueue(NodeId),
    LoadAllItemsToQueue,
    CountriesFetched(Vec<Country>),
    LanguagesFetched(Vec<Language>),
    StationsFetched(Vec<Station>),
    TagsFetched(Vec<Tag>),
    ChangeCategory(FilterType),
    ExpandNodeClick(NodeId),
    CollapseNodeClick(NodeId),
    WebSocketOpen,
    SearchInputChanged(String),
    DoSearch,
    ClearSearch,
    StatusChangeEventReceived(StateChangeEvent),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum FilterType {
    Favorites,
    Country,
    Language,
    Tag,
    Search,
}

#[derive(Debug)]
struct TreeModel {
    arena: Arena<TreeNode>,
    root: NodeId,
    current: NodeId,
}

impl TreeModel {
    fn new() -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(TreeNode::Root);
        TreeModel {
            arena,
            root,
            current: root,
        }
    }
}

#[derive(Debug)]
pub struct Model {
    wait_response: bool,
    filter_type: FilterType,
    tree: TreeModel,
    search_input: String,
}

#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
        MetadataCommand::QueryFavoriteRadioStations,
    )));
    Model {
        wait_response: false,
        filter_type: FilterType::Favorites,
        tree: TreeModel::new(),
        search_input: String::new(),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        ChangeCategory(filter_type) => {
            match &filter_type {
                FilterType::Country => {
                    orders.perform_cmd(async { CountriesFetched(fetch_countries().await) });
                }
                FilterType::Language => {
                    orders.perform_cmd(async { LanguagesFetched(fetch_languages().await) });
                }
                FilterType::Tag => {
                    orders.perform_cmd(async { TagsFetched(fetch_tags().await) });
                }
                FilterType::Favorites => {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                        MetadataCommand::QueryFavoriteRadioStations,
                    )));
                }
                FilterType::Search => {
                    orders.send_msg(Msg::DoSearch);
                }
            }
            model.wait_response = true;
            model.filter_type = filter_type;
        }

        CountriesFetched(list) => {
            model.wait_response = false;
            model.tree = TreeModel::new();

            list.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(TreeNode::Country(item));
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        LanguagesFetched(list) => {
            model.wait_response = false;
            model.tree = TreeModel::new();
            list.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(TreeNode::Language(item));
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        TagsFetched(list) => {
            model.wait_response = false;
            model.tree = TreeModel::new();
            list.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(TreeNode::Tag(item));
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        StationsFetched(list) => {
            model.wait_response = false;
            model.tree = TreeModel::new();
            list.into_iter().for_each(|item| {
                let node = model.tree.arena.new_node(TreeNode::Station(item));
                model.tree.current.append(node, &mut model.tree.arena);
            });
        }
        Msg::StatusChangeEventReceived(StateChangeEvent::FavoriteRadioStations(event)) => {
            model.wait_response = false;
            // model.tree = TreeModel::new();
            orders.perform_cmd(async { StationsFetched(fetch_stations_by_uuid(event).await) });
        }

        ExpandNodeClick(id) => {
            model.wait_response = true;
            model.tree.current = id;

            let node = model.tree.arena.get(id).unwrap().get();
            match node {
                TreeNode::Country(country) => {
                    let cntry = country.iso_3166_1.clone();
                    orders.perform_cmd(
                        async move { StationsFetched(fetch_stations("bycountrycodeexact", &cntry).await) },
                    );
                }
                TreeNode::Language(language) => {
                    let lang = language.name.clone();
                    orders.perform_cmd(async move { StationsFetched(fetch_stations("bylanguageexact", &lang).await) });
                }
                TreeNode::Tag(tag) => {
                    let tag = tag.name.clone();
                    orders.perform_cmd(async move { StationsFetched(fetch_stations("bytagexact", &tag).await) });
                }
                _ => {}
            }
        }
        CollapseNodeClick(id) => {
            let arena = model.tree.arena.clone();
            let children = id.children(&arena);
            for c in children {
                c.remove_subtree(&mut model.tree.arena);
            }
        }

        AddItemToQueue(id) => {
            let node = model.tree.arena.get(id).unwrap().get();
            if let TreeNode::Station(station) = node {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(QueueCommand::AddSongToQueue(
                    station.url.clone(),
                ))));
            }
        }
        LoadItemToQueue(id) => {
            let node = model.tree.arena.get(id).unwrap().get();
            if let TreeNode::Station(station) = node {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(QueueCommand::LoadSongToQueue(
                    station.url.clone(),
                ))));
            }
        }
        Msg::LoadAllItemsToQueue => {
            model.tree.arena.iter().for_each(|node| {
                if let TreeNode::Station(station) = node.get() {
                    orders.send_msg(Msg::SendUserCommand(UserCommand::Queue(QueueCommand::AddSongToQueue(
                        station.url.clone(),
                    ))));
                }
            });
        }
        Msg::FavoriteRadioStation(id) => {
            let node = model.tree.arena.get(id).unwrap().get();
            if let TreeNode::Station(station) = node {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                    MetadataCommand::LikeMediaItem(format!("radio_uuid_{}", station.stationuuid)),
                )));
            }
        }
        Msg::UnfavoriteRadioStation(id) => {
            let node = model.tree.arena.get(id).unwrap().get();
            if let TreeNode::Station(station) = node {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                    MetadataCommand::DislikeMediaItem(format!("radio_uuid_{}", station.stationuuid)),
                )));
            }
            id.remove(&mut model.tree.arena);
        }
        Msg::SearchInputChanged(term) => {
            orders.skip();
            model.search_input = term;
        }
        Msg::DoSearch => {
            model.wait_response = true;
            model.filter_type = FilterType::Search;
            let search_term = model.search_input.clone();
            orders.perform_cmd(async move { StationsFetched(search_stations_by_name(&search_term).await) });
        }
        Msg::ClearSearch => {
            model.wait_response = true;
            model.tree = TreeModel::new();
            model.search_input = String::new();
            orders.send_msg(Msg::ChangeCategory(FilterType::Country));
        }

        _ => {
            orders.skip();
        }
    }
}

pub fn view(model: &Model) -> Node<Msg> {
    section![
        view_spinner_modal(model.wait_response),
        view_search_input(model),
        ul![
            C!["wtree"],
            view_tree(model.tree.root, &model.tree.arena, &model.filter_type)
        ]
    ]
}
fn view_search_input(model: &Model) -> Node<Msg> {
    div![
        C!["transparent is-flex is-justify-content-center has-background-dark-transparent mt-2"],
        div![
            C!["control"],
            input![
                C!["input", "input-size"],
                attrs! {
                    At::Value => model.search_input,
                    At::Name => "search",
                    At::Type => "text",
                    At::Placeholder => "Find radio station by name",
                },
                input_ev(Ev::Input, Msg::SearchInputChanged),
                ev(Ev::KeyDown, |keyboard_event| {
                    if keyboard_event.value_of().to_string() == "[object KeyboardEvent]" {
                        let kev: KeyboardEvent = keyboard_event.unchecked_into();
                        IF!(kev.key_code() == 13 => Msg::DoSearch)
                    } else {
                        None
                    }
                }),
            ],
        ],
        div![
            C!["control"],
            a![
                C!["ml-2"],
                attrs!(At::Title =>"Search"),
                i![C!["material-icons", "is-large-icon", "white-icon"], "search"],
                ev(Ev::Click, move |_| Msg::DoSearch)
            ],
            a![
                C!["ml-2"],
                attrs!(At::Title =>"Clear search"),
                i![C!["material-icons", "is-large-icon", "white-icon"], "backspace"],
                ev(Ev::Click, move |_| Msg::ClearSearch)
            ],
            a![
                C!["ml-4"],
                attrs!(At::Title =>"Add all items to queue"),
                i![
                    C!["material-icons", "is-large-icon", "white-icon"],
                    "playlist_add"
                ],
                ev(Ev::Click, move |_| Msg::LoadAllItemsToQueue)
            ],
        ],
    ]
}
fn view_filter(filter_type: &FilterType) -> Node<Msg> {
    div![
        C!["control"],
        label![
            C!["radio"],
            input![
                attrs! {
                    At::Type => "radio",
                    At::Name => "filter",
                },
                IF!(filter_type == &FilterType::Favorites => attrs! { At::Checked => "checked" }),
                ev(Ev::Change, |_| ChangeCategory(FilterType::Favorites)),
            ],
            "Favorites"
        ],
        label![
            C!["radio"],
            input![
                attrs! {
                    At::Type => "radio",
                    At::Name => "filter",
                },
                IF!(filter_type == &FilterType::Country => attrs! { At::Checked => "checked" }),
                ev(Ev::Change, |_| ChangeCategory(FilterType::Country)),
            ],
            "Countries"
        ],
        label![
            C!["radio"],
            input![
                attrs! {
                    At::Type => "radio",
                    At::Name => "filter",

                },
                IF!(filter_type == &FilterType::Language => attrs! { At::Checked => "checked" }),
                ev(Ev::Change, |_| ChangeCategory(FilterType::Language)),
            ],
            "Languages"
        ],
        label![
            C!["radio"],
            input![
                attrs! {
                    At::Type => "radio",
                    At::Name => "filter",
                },
                IF!(filter_type == &FilterType::Tag => attrs! { At::Checked => "checked" }),
                ev(Ev::Change, |_| ChangeCategory(FilterType::Tag)),
            ],
            "Tags"
        ]
    ]
}

#[allow(clippy::collection_is_never_read)]
fn view_tree(node_id: NodeId, arena: &Arena<TreeNode>, filter_type: &FilterType) -> Node<Msg> {
    let Some(value) = arena.get(node_id) else {
        return empty!();
    };
    let item = value.get();
    let children: Vec<NodeId> = node_id.children(arena).collect();
    let mut li: Node<Msg> = li![];
    let node_height = "40px";
    let mut label = String::new();
    let mut is_dir = false;
    let mut is_root = false;
    let mut favicon = String::new();
    let is_favorites = matches!(filter_type, FilterType::Favorites);
    let (fav_icon, fav_action) = if is_favorites {
        ("favorite", Msg::UnfavoriteRadioStation(node_id))
    } else {
        ("favorite_border", Msg::FavoriteRadioStation(node_id))
    };
    match item {
        TreeNode::Country(country) => {
            label = format!("{} ({})", country.name, country.stationcount);
            is_dir = true;
        }
        TreeNode::Language(language) => {
            label = format!("{} ({})", language.name, language.stationcount);
            is_dir = true;
        }
        TreeNode::Tag(tag) => {
            label = format!("{} ({})", tag.name, tag.stationcount);
            is_dir = true;
        }
        TreeNode::Station(station) => {
            label = format!(
                "{} (votes:{} / codec:{} / bitrate:{})",
                station.name, station.votes, station.codec, station.bitrate
            );
            favicon.clone_from(&station.favicon);
        }
        TreeNode::Root => {
            is_root = true;
        }
    };
    let mut span: Node<Msg> = span![
        C!["has-background-dark-transparent"],
        style! {
            St::Height => node_height,
        },
        IF!(is_root => view_filter(filter_type)),
        IF!(is_root => style! { St::Padding => "5px" }),
    ];

    if !is_root {
        let left_position = if is_dir {
            "20px"
        } else if !favicon.is_empty() {
            node_height
        } else {
            "0px"
        };
        span.add_child(div![
            C!["level", "is-mobile"],
            div![
                C!["level-left", "is-flex-grow-3"],
                style! {
                    St::Height => node_height,
                },
                IF!(is_dir =>

                    if children.is_empty() {
                        i![C!["material-icons"], "expand_more"]
                    } else {
                        i![C!["material-icons"], "expand_less"]
                    }
                ),
                IF!(is_dir =>
                    if children.is_empty() {
                            ev(Ev::Click, move |_| Msg::ExpandNodeClick(node_id))
                    } else {
                            ev(Ev::Click, move |_| Msg::CollapseNodeClick(node_id))
                    }
                ),
                IF!(!is_dir && !favicon.is_empty() =>
                    img![
                        C!["is-rounded"],
                        style! {
                            St::Height => node_height,
                            St::Width => node_height,
                            St::Padding => "5px",
                        },
                        attrs! {
                            At::Src => favicon,
                        }
                    ]
                ),
                p![
                    C!["level-item"],
                    style! {
                        St::Position => "absolute",
                        St::Left => left_position,
                        St::Padding => "5px",
                        St::TextOverflow => "ellipsis",
                        St::Overflow => "hidden",
                        St::WhiteSpace => "nowrap",
                    },
                    label
                ],
            ],
            IF!(!is_dir =>
            div![
                C!["level-right"],
                div![
                    C!["level-item", "mr-2"],
                    i![C!["material-icons"], fav_icon],
                    ev(Ev::Click, move |_| fav_action)
                ],
                div![
                    C!["level-item", "mr-2"],
                    i![C!["material-icons"], "playlist_add"],
                    ev(Ev::Click, move |_| AddItemToQueue(node_id))
                ],
                div![
                    C!["level-item", "mr-2"],
                    i![C!["material-icons"], "play_circle_filled"],
                    ev(Ev::Click, move |_| LoadItemToQueue(node_id))
                ],
            ]
            )
        ]);
    }

    li.add_child(span);
    if !children.is_empty() {
        let mut ul: Node<Msg> = ul!();
        for c in children {
            ul.add_child(view_tree(c, arena, filter_type));
        }
        li.add_child(ul);
    }
    li
}

const RADIO_BROWSER_URL: &str = "https://de1.api.radio-browser.info/json/";

#[allow(clippy::future_not_send)]
async fn search_stations_by_name(name: &str) -> Vec<Station> {
    let url = format!(
        "{}stations/search?name={}&limit=300&hidebroken=true",
        RADIO_BROWSER_URL, name
    );
    Request::get(&url)
        .send()
        .await
        .unwrap()
        .json::<Vec<Station>>()
        .await
        .unwrap()
}

#[allow(clippy::future_not_send)]
async fn fetch_countries() -> Vec<Country> {
    let url = format!("{}countries?limit=200&hidebroken=true", RADIO_BROWSER_URL);
    let Ok(response) = Request::get(&url).send().await else {
        return vec![];
    };
    response.json::<Vec<Country>>().await.unwrap()
}

#[allow(clippy::future_not_send)]
async fn fetch_stations_by_uuid(uuids: Vec<String>) -> Vec<Station> {
    let url = format!("{}stations/byuuid?uuids={}", RADIO_BROWSER_URL, uuids.join(","));
    let Ok(response) = Request::get(&url).send().await else {
        return vec![];
    };
    response.json::<Vec<Station>>().await.unwrap()
}

#[allow(clippy::future_not_send)]
async fn fetch_stations(by: &str, value: &str) -> Vec<Station> {
    let url = format!(
        "{}stations/{by}/{}?limit=300&hidebroken=true&order=votes&reverse=true",
        RADIO_BROWSER_URL, value
    );
    let Ok(response) = Request::get(&url).send().await else {
        return vec![];
    };
    response.json::<Vec<Station>>().await.unwrap()
}
#[allow(clippy::future_not_send)]
async fn fetch_languages() -> Vec<Language> {
    let url = format!("{}languages?limit=500", RADIO_BROWSER_URL);
    let Ok(response) = Request::get(&url).send().await else {
        return vec![];
    };
    response.json::<Vec<Language>>().await.unwrap()
}
#[allow(clippy::future_not_send)]
async fn fetch_tags() -> Vec<Tag> {
    let url = format!(
        "{}tags?limit=500&order=stationcount&reverse=true&hidebroken=true",
        RADIO_BROWSER_URL
    );

    let Ok(response) = Request::get(&url).send().await else {
        return vec![];
    };
    response.json::<Vec<Tag>>().await.unwrap()
}

#[cfg(test)]
mod test {
    use gloo_console::log;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);
    #[wasm_bindgen_test]
    async fn test_fetch_countries() {
        let cnt = super::fetch_countries().await;
        log!("cnt len", cnt.len());
        log!("name", &cnt[0].name);
        log!("name", &cnt[0].iso_3166_1);
        assert!(!cnt.is_empty());
    }

    #[wasm_bindgen_test]
    async fn test_fetch_stations() {
        let stations = super::fetch_stations("bycountrycodeexact", "DE").await;
        log!("stations:", stations.len());
        stations.iter().take(5).for_each(|s| {
            log!("name", &s.name);
            log!("url", &s.url);
            log!("favicon", &s.favicon);
            log!("tags", &s.tags);
            log!("language", &s.language);
            log!("state", &s.state);
            // log!("votes", &s.votes);
            log!("codec", &s.codec);
            // log!("bitrate", &s.bitrate);
        });
        assert!(!stations.is_empty());
    }

    #[wasm_bindgen_test]
    async fn test_fetch_languages() {
        let languages = super::fetch_languages().await;
        log!("languages:", languages.len());
        languages.iter().take(5).for_each(|s| {
            log!("name", &s.name);
        });
        assert!(!languages.is_empty());
    }
}
