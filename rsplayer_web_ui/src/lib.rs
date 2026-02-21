extern crate api_models;

use std::{rc::Rc, str::FromStr};

use api_models::{
    common::{MetadataCommand, PlaybackMode, PlayerCommand, QueueCommand, SystemCommand, UserCommand, Volume},
    player::Song,
    state::{PlayerInfo, PlayerState, SongProgress, StateChangeEvent},
};
use gloo_console::{error, log};
use gloo_net::http::Request;
use seed::{prelude::*, *};
use serde::Deserialize;
use strum_macros::IntoStaticStr;
use wasm_sockets::{self, ConnectionStatus, EventClient, Message, WebSocketError};
use web_sys::CloseEvent;
use PlayerCommand::{CyclePlaybackMode, Next, Pause, Play, Prev};
use UserCommand::{Player, Queue};

mod dsp;
mod page;
mod vumeter;

const SETTINGS: &str = "settings";

const QUEUE: &str = "queue";
const FIRST_SETUP: &str = "setup";
const PLAYER: &str = "player";
const MUSIC_LIBRARY: &str = "library";
const MUSIC_LIBRARY_FILES: &str = "files";
const MUSIC_LIBRARY_ARTISTS: &str = "artists";
const MUSIC_LIBRARY_RADIO: &str = "radio";
const MUSIC_LIBRARY_PL_STATIC: &str = "playlists";

// ------ ------
//     Model
// ------ ------
#[derive(Debug)]
pub struct PlayerModel {
    volume_state: Volume,
    player_info: Option<PlayerInfo>,
    current_song: Option<Song>,
    progress: SongProgress,
    playback_mode: PlaybackMode,
    player_state: PlayerState,
    stop_updates: bool,
    vu_meter_enabled: bool,
}

// #[derive(Debug)]
struct Model {
    base_url: Url,
    page: Page,
    web_socket: EventClient,
    web_socket_reconnector: Option<StreamHandle>,
    startup_error: Option<String>,
    player_model: PlayerModel,
    metadata_scan_info: Option<String>,
    notification: Option<StateChangeEvent>,
    vumeter: Option<vumeter::VUMeter>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Msg {
    WebSocketOpened,
    WebSocketMessageReceived(String),
    WebSocketClosed(CloseEvent),
    WebSocketFailed,
    ReconnectWebSocket,
    UrlChanged(subs::UrlChanged),
    StatusChangeEventReceived(StateChangeEvent),
    Settings(page::settings::Msg),
    StartErrorReceived(String),
    Queue(page::queue::Msg),
    MusicLibraryStaticPlaylist(page::music_library_static_playlist::Msg),
    MusicLibraryFiles(page::music_library_files::Msg),
    MusicLibraryArtists(page::music_library_artists::Msg),
    MusicLibraryRadio(page::music_library_radio::Msg),
    Ignore,

    SendUserCommand(UserCommand),
    SendSystemCommand(SystemCommand),
    AlbumImageUpdated(Image),
    HideMetadataScanInfo,
    HideNotification,
    ReloadApp,

    SeekTrackPosition(u16),
    SeekTrackPositionInput(u16),
    ResumeUpdates,
    SetVolume(String),
    SetVolumeInput(String),
    InitVUMeter,
    WindowResized,

    LikeMediaItemClick(MetadataCommand),
}

#[derive(Debug, Deserialize)]
pub struct AlbumInfo {
    pub album: Album,
}

#[derive(Debug, Deserialize)]
pub struct Album {
    image: Vec<Image>,
}

#[derive(Debug, Deserialize)]
pub struct Image {
    size: String,
    #[serde(rename = "#text")]
    text: String,
}

// ------ Page ------
#[derive(Debug, IntoStaticStr)]
#[allow(clippy::large_enum_variant)]
enum Page {
    Home,
    Settings(page::settings::Model),
    Player,
    Queue(page::queue::Model),
    MusicLibraryStaticPlaylist(page::music_library_static_playlist::Model),
    MusicLibraryFiles(page::music_library_files::Model),
    MusicLibraryArtists(page::music_library_artists::Model),
    MusicLibraryRadio(page::music_library_radio::Model),
    NotFound,
}

impl Page {
    fn new(url: Url, orders: &mut impl Orders<Msg>) -> Self {
        let mut iter = url.hash_path().iter();
        let first_level = iter.next().map_or("", |v| v.as_str());
        let second_level_raw = iter.next().map_or("", |v| v.as_str());
        let second_level = second_level_raw.split('?').next().unwrap_or(second_level_raw);
        match first_level {
            FIRST_SETUP => Self::Home,
            SETTINGS => Self::Settings(page::settings::init(url, &mut orders.proxy(Msg::Settings))),

            QUEUE => Self::Queue(page::queue::init(url, &mut orders.proxy(Msg::Queue))),
            MUSIC_LIBRARY => match second_level {
                MUSIC_LIBRARY_FILES => Self::MusicLibraryFiles(page::music_library_files::init(
                    url,
                    &mut orders.proxy(Msg::MusicLibraryFiles),
                )),
                MUSIC_LIBRARY_RADIO => Self::MusicLibraryRadio(page::music_library_radio::init(
                    url,
                    &mut orders.proxy(Msg::MusicLibraryRadio),
                )),
                MUSIC_LIBRARY_ARTISTS => Self::MusicLibraryArtists(page::music_library_artists::init(
                    url,
                    &mut orders.proxy(Msg::MusicLibraryArtists),
                )),
                MUSIC_LIBRARY_PL_STATIC => Self::MusicLibraryStaticPlaylist(page::music_library_static_playlist::init(
                    url,
                    &mut orders.proxy(Msg::MusicLibraryStaticPlaylist),
                )),
                _ => Self::NotFound,
            },
            PLAYER | "" => Self::Player,
            _ => Self::NotFound,
        }
    }

    const fn has_image_background(&self) -> bool {
        !matches!(self, Page::Settings(_))
    }
    const fn has_tabs(&self) -> bool {
        matches!(
            self,
            Page::MusicLibraryFiles(_)
                | Page::MusicLibraryStaticPlaylist(_)
                | Page::MusicLibraryArtists(_)
                | Page::MusicLibraryRadio(_)
        )
    }
}

// ------ ------
//     Init
// ------ ------
#[allow(clippy::needless_pass_by_value)]
fn init(url: Url, orders: &mut impl Orders<Msg>) -> Model {
    let page = Page::new(url.clone(), orders);
    orders.subscribe(Msg::UrlChanged).notify(subs::UrlChanged(url.clone()));
    orders.stream(streams::window_event(Ev::Resize, |_| Msg::WindowResized));

    if matches!(page, Page::Player) {
        orders.after_next_render(|_| Some(Msg::InitVUMeter));
    }

    orders.perform_cmd(async {
        let response = Request::get("/api/start_error")
            .send()
            .await
            .expect("failed to get response");
        if response.ok() {
            Msg::StartErrorReceived(response.text().await.expect(""))
        } else {
            Msg::Ignore
        }
    });
    Model {
        base_url: url.to_base_url(),
        page,
        web_socket: create_websocket(orders).unwrap(),
        web_socket_reconnector: None,
        startup_error: None,
        player_model: PlayerModel {
            volume_state: Volume::default(),
            player_info: None,
            current_song: None,
            progress: SongProgress::default(),
            playback_mode: PlaybackMode::default(),
            player_state: PlayerState::STOPPED,
            stop_updates: false,
            vu_meter_enabled: false,
        },
        metadata_scan_info: None,
        notification: None,
        vumeter: None,
    }
}
// ------ ------
//     Urls
// ------ ------

struct_urls!();
impl<'a> Urls<'a> {
    fn settings(self) -> Url {
        self.base_url().add_hash_path_part(SETTINGS)
    }
    fn settings_abs() -> Url {
        Url::new().add_hash_path_part(SETTINGS)
    }
    fn queue_abs() -> Url {
        Url::new().add_hash_path_part(QUEUE)
    }

    fn player_abs() -> Url {
        Url::new().add_hash_path_part(PLAYER)
    }
    fn library_abs() -> Url {
        Url::new().add_hash_path_part(MUSIC_LIBRARY)
    }
    fn get_search_term(url: &Url) -> Option<String> {
        url.hash_path().iter().find_map(|p| {
            log!("p", p);
            if p.contains("?search=") {
                let term = p
                    .split_once("?search=")
                    .map(|(_, term)| term.to_string())
                    .unwrap_or(p.to_string());
                log!("term", &term);
                Some(term)
            } else {
                None
            }
        })
    }
}

// ------ ------
//    Update
// ------ ------
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::WebSocketOpened => {
            model.web_socket_reconnector = None;
            log!("WebSocket connection is open now");
            orders.send_msg(Msg::SendUserCommand(Queue(QueueCommand::QueryCurrentSong)));
            orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::QueryCurrentPlayerInfo)));
            orders.send_msg(Msg::SendSystemCommand(SystemCommand::QueryCurrentVolume));
            if let Page::Queue(model) = &mut model.page {
                page::queue::update(page::queue::Msg::WebSocketOpen, model, &mut orders.proxy(Msg::Queue));
            }
            if let Page::MusicLibraryFiles(model) = &mut model.page {
                page::music_library_files::update(
                    page::music_library_files::Msg::WebSocketOpen,
                    model,
                    &mut orders.proxy(Msg::MusicLibraryFiles),
                );
            }
            if let Page::MusicLibraryArtists(model) = &mut model.page {
                page::music_library_artists::update(
                    page::music_library_artists::Msg::WebSocketOpen,
                    model,
                    &mut orders.proxy(Msg::MusicLibraryArtists),
                );
            }
            if let Page::MusicLibraryRadio(model) = &mut model.page {
                page::music_library_radio::update(
                    page::music_library_radio::Msg::WebSocketOpen,
                    model,
                    &mut orders.proxy(Msg::MusicLibraryRadio),
                );
            }
            if let Page::MusicLibraryStaticPlaylist(model) = &mut model.page {
                page::music_library_static_playlist::update(
                    page::music_library_static_playlist::Msg::WebSocketOpen,
                    model,
                    &mut orders.proxy(Msg::MusicLibraryStaticPlaylist),
                );
            }
        }

        Msg::WebSocketClosed(close_event) => {
            log!("==================");
            log!("WebSocket connection was closed:");
            log!("Clean:", close_event.was_clean());
            log!("Code:", close_event.code());
            log!("Reason:", close_event.reason());
            log!("==================");

            // Chrome doesn't invoke `on_error` when the connection is lost.
            if !close_event.was_clean() && model.web_socket_reconnector.is_none() {
                orders.after_next_render(|_| scrollToId("reconnectinfo"));
                model.web_socket_reconnector =
                    Some(orders.stream_with_handle(streams::interval(2000, || Msg::ReconnectWebSocket)));
            }
        }

        Msg::WebSocketFailed => {
            log!("WebSocket failed");
            if model.web_socket_reconnector.is_none() {
                orders.after_next_render(|_| scrollToId("reconnectinfo"));
                model.web_socket_reconnector =
                    Some(orders.stream_with_handle(streams::interval(1000, || Msg::ReconnectWebSocket)));
            }
        }

        Msg::ReconnectWebSocket => {
            model.web_socket = create_websocket(orders).unwrap();
        }

        Msg::UrlChanged(subs::UrlChanged(url)) => {
            model.page = Page::new(url, orders);
            if matches!(model.page, Page::Player) && model.player_model.vu_meter_enabled {
                orders.after_next_render(|_| Some(Msg::InitVUMeter));
            } else {
                model.vumeter = None;
            }
        }

        Msg::AlbumImageUpdated(image) => {
            model.player_model.current_song.as_mut().unwrap().image_url = Some(image.text);
        }
        Msg::SeekTrackPosition(pos) => {
            log!(format!("Seeking to {}", pos));
            model.player_model.stop_updates = true;
            model.player_model.progress.current_time = std::time::Duration::from_secs(pos.into());
            orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::Seek(pos))));
            orders.perform_cmd(cmds::timeout(100, || Msg::ResumeUpdates));
        }
        Msg::SeekTrackPositionInput(pos) => {
            model.player_model.stop_updates = true;
            model.player_model.progress.current_time = std::time::Duration::from_secs(pos.into());
        }
        Msg::ResumeUpdates => {
            log!("Resuming progress updates");
            model.player_model.stop_updates = false;
            orders.skip();
        }
        Msg::InitVUMeter => {
            if model.player_model.vu_meter_enabled {
                if let Some(meter) = vumeter::VUMeter::new("vumeter") {
                    model.vumeter = Some(meter);
                }
            } else {
                model.vumeter = None;
            }
        }
        Msg::WindowResized => {
            if let Some(meter) = &mut model.vumeter {
                meter.resize();
            }
        }
        Msg::SetVolume(volstr) => {
            log!("New vol string {}", &volstr);
            model.player_model.stop_updates = true;
            let vol = u8::from_str(volstr.as_str()).unwrap_or(model.player_model.volume_state.current);
            model.player_model.volume_state.current = vol;
            orders.send_msg(Msg::SendSystemCommand(SystemCommand::SetVol(vol)));
            orders.perform_cmd(cmds::timeout(100, || Msg::ResumeUpdates));
        }
        Msg::SetVolumeInput(volstr) => {
            model.player_model.stop_updates = true;
            let vol = u8::from_str(volstr.as_str()).unwrap_or(model.player_model.volume_state.current);
            model.player_model.volume_state.current = vol;
            orders.perform_cmd(cmds::timeout(100, || Msg::ResumeUpdates));
        }
        Msg::LikeMediaItemClick(MetadataCommand::LikeMediaItem(item_id)) => {
            if let Some(song) = model.player_model.current_song.as_mut() {
                if let Some(stats) = song.statistics.as_mut() {
                    stats.liked_count = 1;
                } else {
                    song.statistics = Some(api_models::stat::PlayItemStatistics {
                        play_item_id: item_id.clone(),
                        liked_count: 1,
                        ..Default::default()
                    });
                }
            }

            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                MetadataCommand::LikeMediaItem(item_id),
            )));
        }
        Msg::LikeMediaItemClick(MetadataCommand::DislikeMediaItem(item_id)) => {
            if let Some(song) = model.player_model.current_song.as_mut() {
                if let Some(stats) = song.statistics.as_mut() {
                    stats.liked_count = 0;
                }
            }
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                MetadataCommand::DislikeMediaItem(item_id),
            )));
        }
        Msg::HideMetadataScanInfo => {
            model.metadata_scan_info = None;
        }
        Msg::HideNotification => {
            model.notification = None;
        }
        Msg::StatusChangeEventReceived(chg_ev) => {
            match &chg_ev {
                StateChangeEvent::CurrentSongEvent(song) => {
                    let ps = song.clone();
                    if ps.image_url.is_none() {
                        orders.perform_cmd(async { update_album_cover(ps).await });
                    }
                    model.player_model.current_song = Some(song.clone());
                }
                StateChangeEvent::VolumeChangeEvent(vol) => {
                    if model.player_model.volume_state.current != vol.current {
                        model.player_model.volume_state = vol.clone();
                    } 
                }
                StateChangeEvent::PlayerInfoEvent(pi) => {
                    model.player_model.player_info = Some(pi.clone());
                }
                StateChangeEvent::PlaybackModeChangedEvent(mode) => {
                    model.player_model.playback_mode = *mode;
                }
                StateChangeEvent::PlaybackStateEvent(ps) => {
                    model.player_model.player_state = ps.clone();
                    if !matches!(ps, PlayerState::PLAYING) {
                        if let Some(meter) = &mut model.vumeter {
                            meter.update(0, 0);
                        }
                    }
                }
                StateChangeEvent::MetadataSongScanStarted => {
                    model.metadata_scan_info = Some("Music directory scanning started.".to_string());
                    orders.after_next_render(|_| scrollToId("scaninfo"));
                }
                StateChangeEvent::MetadataSongScanned(info) => {
                    model.metadata_scan_info = Some(info.clone());
                }
                StateChangeEvent::MetadataSongScanFinished(info) => {
                    model.metadata_scan_info = Some(info.clone());
                    orders.perform_cmd(cmds::timeout(5000, || Msg::HideMetadataScanInfo));
                    orders.after_next_render(|_| scrollToId("scaninfo"));
                }
                StateChangeEvent::NotificationSuccess(_) | StateChangeEvent::NotificationError(_) => {
                    model.notification = Some(chg_ev.clone());
                    orders.perform_cmd(cmds::timeout(4000, || Msg::HideNotification));
                    orders.skip();
                }
                StateChangeEvent::VUEvent(l, r) => {
                    if model.player_model.stop_updates {
                        orders.skip();
                        return;
                    }
                    if let Some(meter) = &mut model.vumeter {
                        meter.update(*l, *r);
                    }
                    orders.skip();
                }
                StateChangeEvent::VuMeterEnabledEvent(enabled) => {
                    model.player_model.vu_meter_enabled = *enabled;
                    if *enabled && matches!(model.page, Page::Player) {
                        orders.after_next_render(|_| Some(Msg::InitVUMeter));
                    } else {
                        model.vumeter = None;
                    }
                }
                _ => {}
            }

            if let Page::Queue(model) = &mut model.page {
                page::queue::update(
                    page::queue::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::Queue),
                );
            } else if let Page::MusicLibraryFiles(model) = &mut model.page {
                page::music_library_files::update(
                    page::music_library_files::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::MusicLibraryFiles),
                );
            } else if let Page::MusicLibraryStaticPlaylist(model) = &mut model.page {
                page::music_library_static_playlist::update(
                    page::music_library_static_playlist::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::MusicLibraryStaticPlaylist),
                );
            } else if let Page::MusicLibraryArtists(model) = &mut model.page {
                page::music_library_artists::update(
                    page::music_library_artists::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::MusicLibraryArtists),
                );
            } else if let Page::MusicLibraryRadio(model) = &mut model.page {
                page::music_library_radio::update(
                    page::music_library_radio::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::MusicLibraryRadio),
                );
            }
        }

        Msg::Settings(msg) => {
            if let Page::Settings(sett_model) = &mut model.page {
                if let page::settings::Msg::SendSystemCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                if let page::settings::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::settings::update(msg, sett_model, &mut orders.proxy(Msg::Settings));
            }
        }

        Msg::MusicLibraryStaticPlaylist(msg) => {
            if let Page::MusicLibraryStaticPlaylist(player_model) = &mut model.page {
                if let page::music_library_static_playlist::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::music_library_static_playlist::update(
                    msg,
                    player_model,
                    &mut orders.proxy(Msg::MusicLibraryStaticPlaylist),
                );
            }
        }

        Msg::Queue(msg) => {
            if let Page::Queue(player_model) = &mut model.page {
                if let page::queue::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::queue::update(msg, player_model, &mut orders.proxy(Msg::Queue));
            }
        }

        Msg::MusicLibraryFiles(msg) => {
            if let Page::MusicLibraryFiles(music_lib_model) = &mut model.page {
                if let page::music_library_files::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::music_library_files::update(msg, music_lib_model, &mut orders.proxy(Msg::MusicLibraryFiles));
            }
        }
        Msg::MusicLibraryArtists(msg) => {
            if let Page::MusicLibraryArtists(music_lib_model) = &mut model.page {
                if let page::music_library_artists::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::music_library_artists::update(msg, music_lib_model, &mut orders.proxy(Msg::MusicLibraryArtists));
            }
        }
        Msg::MusicLibraryRadio(msg) => {
            if let Page::MusicLibraryRadio(music_lib_model) = &mut model.page {
                if let page::music_library_radio::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::music_library_radio::update(msg, music_lib_model, &mut orders.proxy(Msg::MusicLibraryRadio));
            }
        }

        Msg::WebSocketMessageReceived(message) => {
            let msg = serde_json::from_str::<StateChangeEvent>(&message)
                .unwrap_or_else(|_| panic!("Failed to decode WebSocket text message: {message}"));
            if let StateChangeEvent::SongTimeEvent(st) = msg {
                if model.player_model.stop_updates {
                    log!("Updates are stopped");
                    return;
                }
                model.player_model.progress = st;
                model.player_model.player_state = PlayerState::PLAYING;
            } else {
                orders.send_msg(Msg::StatusChangeEventReceived(msg));
            }
        }
        Msg::StartErrorReceived(error_msg) => {
            model.startup_error = Some(error_msg);
        }
        Msg::SendUserCommand(cmd) => {
            _ = model.web_socket.send_string(&serde_json::to_string(&cmd).unwrap());
        }
        Msg::SendSystemCommand(cmd) => {
            _ = model.web_socket.send_string(&serde_json::to_string(&cmd).unwrap());
            orders.skip();
        }
        Msg::ReloadApp => {
            _ = window().location().reload();
        }
        Msg::Ignore => {}
        _ => {}
    }
}

// ------ ------
//     View
// ------ ------

fn view(model: &Model) -> impl IntoNodes<Msg> {
    let mut bg = "radial-gradient(circle, rgb(49, 144, 228) 0%, rgb(0, 0, 0) 100%);".to_string();
    if model.page.has_image_background() {
        if let Some(bg_image) = get_background_image(&model.player_model) {
            bg = format!("url({})", bg_image)
        }
    };

    div![
        style! {
            St::BackgroundImage => bg,
            St::BackgroundRepeat => "no-repeat",
            St::BackgroundSize => "cover",
            St::MinHeight => "95vh",
        },
        C!["container"],
        view_navigation_tabs(&model.page),
        view_startup_error(model.startup_error.as_ref()),
        view_reconnect_notification(model),
        view_metadata_scan_notification(model),
        view_content(model, &model.base_url),
        view_player_footer(&model.page, &model.player_model),
        view_notification(model)
    ]
}

fn view_reconnect_notification(model: &Model) -> Node<Msg> {
    if model.web_socket_reconnector.is_some() {
        div![
            id!("reconnectinfo"),
            C!["notification", "is-danger"],
            p!["Connection to sever failed. Reconnecting..."],
            p!["Please wait or refresh page if problem persists."],
            // button![C!["button", "is-info", "ml-6", "is-outlined"], "Reload",
            //     ev(Ev::Click, |_| Msg::ReloadApp)
            //  ]
        ]
    } else {
        empty!()
    }
}

fn view_notification(model: &Model) -> Node<Msg> {
    div![
        style! {
            "z-index" => 10
            "bottom" => 0
            "position" => "fixed"
        },
        model.notification.as_ref().map_or(empty!(), |not| match not {
            StateChangeEvent::NotificationSuccess(info) => {
                div![C!["notification", "is-primary", "is-light"], info]
            }
            StateChangeEvent::NotificationError(error) => {
                div![C!["notification", "is-error", "is-light"], error]
            }
            _ => empty!(),
        })
    ]
}

fn view_metadata_scan_notification(model: &Model) -> Node<Msg> {
    model.metadata_scan_info.as_ref().map_or_else(
        || empty!(),
        |info| {
            div![
                id!("scaninfo"),
                C!["notification", "is-info", "is-light"],
                button![C!("delete")],
                p!["Music directory scan is running..."],
                p![
                    C!["has-overflow-ellipsis-text"],
                    style! {
                        St::MaxWidth => "95%"
                    },
                    info
                ]
            ]
        },
    )
}

#[allow(clippy::too_many_lines)]
fn view_player_footer(page: &Page, player_model: &PlayerModel) -> Node<Msg> {
    if matches!(page, Page::Player) {
        return empty!();
    };
    let playing = player_model.player_state == PlayerState::PLAYING;

    let (shuffle_class, shuffle_title) = match player_model.playback_mode {
        PlaybackMode::Sequential => ("fa-list-ol", "Sequential Playback"),
        PlaybackMode::Random => ("fa-shuffle", "Random Playback"),
        PlaybackMode::LoopSingle => ("fa-repeat", "Loop Single Song"),
        PlaybackMode::LoopQueue => ("fa-arrows-rotate", "Loop Queue"),
    };

    div![
        C!["page-foot", "container"],
        nav![
            style! {
                St::Width => "100%"
            },
            C!["level", "is-mobile"],
            // image
            div![
                C![
                    "level-left",
                    "is-flex-grow-1",
                    "is-hidden-mobile",
                    "is-clickable",
                    "mr-2"
                ],
                div![
                    C!["level-item"],
                    figure![
                        C!["image", "is-64x64"],
                        img![
                            attrs! {"src" => get_background_image(player_model).unwrap_or("/headphones.png".to_string())}
                        ],
                    ]
                ],
                ev(Ev::Click, |_| { Urls::player_abs().go_and_load() })
            ],
            // track info
            div![
                C!["level-left", "is-flex-grow-3", "is-clickable"],
                div![
                    C!["level-item", "is-justify-content-flex-start", "available-width"],
                    div![
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model
                                .current_song
                                .as_ref()
                                .map_or(String::new(), api_models::player::Song::get_title)
                        ],
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model.current_song.as_ref().map_or(String::new(), |fa| fa
                                .album
                                .as_ref()
                                .map_or(String::new(), std::clone::Clone::clone))
                        ],
                        p![
                            C!["heading", "has-overflow-ellipsis-text"],
                            player_model.current_song.as_ref().map_or(String::new(), |fa| fa
                                .artist
                                .as_ref()
                                .map_or(String::new(), std::clone::Clone::clone))
                        ],
                    ]
                ],
                ev(Ev::Click, |_| { Urls::player_abs().go_and_load() })
            ],
            // track progress
            div![
                C!["level-left", "is-flex-grow-5", "is-hidden-mobile"],
                div![
                    C!["level-item"],
                    div![
                        C!["has-text-centered", "available-width"],
                        span![
                            C!["is-size-6", "has-text-light", "has-background-dark-transparent"],
                            player_model.progress.format_time()
                        ],
                        progress![
                            C!["progress", "is-small"],
                            attrs! {"value"=> player_model.progress.current_time.as_secs()},
                            attrs! {"max"=> player_model.progress.total_time.as_secs()},
                            player_model.progress.current_time.as_secs()
                        ],
                    ]
                ]
            ],
            // player controls
            div![
                C!["level-right", "is-flex-grow-3"],
                div![
                    C!["level-item", "is-justify-content-flex-end"],
                    div![
                        div![
                            i![
                                C!["fas", "is-clickable", "small-button-footer", shuffle_class],
                                attrs! {At::Title => shuffle_title},
                                ev(Ev::Click, |_| Msg::SendUserCommand(Player(CyclePlaybackMode))),
                            ],
                            i![
                                C!["fas", "is-clickable", "fa-backward", "small-button-footer"],
                                ev(Ev::Click, |_| Msg::SendUserCommand(Player(Prev))),
                            ],
                            i![
                                C![
                                    "fa",
                                    "is-clickable",
                                    "small-button-footer",
                                    IF!(playing => "fa-pause" ),
                                    IF!(!playing => "fa-play" )
                                ],
                                ev(Ev::Click, move |_| if playing {
                                    Msg::SendUserCommand(Player(Pause))
                                } else {
                                    Msg::SendUserCommand(Player(Play))
                                })
                            ],
                            i![
                                C!["fas", "is-clickable", "fa-forward", "small-button-footer"],
                                ev(Ev::Click, |_| Msg::SendUserCommand(Player(Next))),
                            ],
                        ],
                        div![input![
                            C!["slider"],
                            attrs! {"value"=> player_model.volume_state.current},
                            // attrs! {"step"=> player_model.volume_state.step},
                            attrs! {"max"=> player_model.volume_state.max},
                            attrs! {"min"=> player_model.volume_state.min},
                            attrs! {"type"=> "range"},
                            input_ev(Ev::Change, Msg::SetVolume),
                        ],],
                    ]
                ],
            ],
            div![
                C!["level-right", "is-flex-grow-1"],
                div![
                    C!["level-item"],
                    i![
                        C!["fas", "is-clickable", "fa-up-right-and-down-left-from-center"],
                        ev(Ev::Click, |_| { Urls::player_abs().go_and_load() })
                    ],
                ]
            ]
        ]
    ]
}

fn view_startup_error(error_msg: Option<&String>) -> Node<Msg> {
    error_msg.map_or(empty!(), |error| {
        article![
            C!["message", "is-danger"],
            div![
                C!["message-header"],
                p![
                    "Startup error! Please check  ",
                    a!["settings.", ev(Ev::Click, |_| { Urls::settings_abs().go_and_load() }),]
                ],
            ],
            div![C!["message-body"], error]
        ]
    })
}
// ----- view_content ------

fn view_content(main_model: &Model, base_url: &Url) -> Node<Msg> {
    let page = &main_model.page;
    div![
        style! {
            St::Background => "rgba(86, 92, 86, 0.507)",
            St::MinHeight => "95vh"
        },
        C!["main-content"],
        IF!(main_model.page.has_tabs() => view_music_lib_tabs(&main_model.page)),
        match page {
            Page::Home => page::home::view(base_url),
            Page::NotFound => page::not_found::view(),
            Page::Settings(model) => page::settings::view(model).map_msg(Msg::Settings),
            Page::Player => page::player::view(&main_model.player_model),
            Page::Queue(model) => page::queue::view(model).map_msg(Msg::Queue),
            Page::MusicLibraryStaticPlaylist(model) =>
                page::music_library_static_playlist::view(model).map_msg(Msg::MusicLibraryStaticPlaylist),
            Page::MusicLibraryFiles(model) => page::music_library_files::view(model).map_msg(Msg::MusicLibraryFiles),
            Page::MusicLibraryArtists(model) =>
                page::music_library_artists::view(model).map_msg(Msg::MusicLibraryArtists),
            Page::MusicLibraryRadio(model) => page::music_library_radio::view(model).map_msg(Msg::MusicLibraryRadio),
        }
    ]
}

fn view_navigation_tabs(page: &Page) -> Node<Msg> {
    let page_name: &str = page.into();
    div![
        C![
            "tabs",
            "is-toggle",
            "is-centered",
            "is-fullwidth",
            "has-background-dark-transparent"
        ],
        ul![
            li![
                IF!(page_name == "Player" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "music_note"],
                ]],
                ev(Ev::Click, |_| { Urls::player_abs().go_and_load() }),
            ],
            li![
                IF!(page_name == "Queue" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "queue_music"],
                ]],
                ev(Ev::Click, |_| { Urls::queue_abs().go_and_load() }),
            ],
            li![
                IF!(page_name == "MusicLibraryFiles" || page_name ==  "MusicLibraryStaticPlaylist" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "library_music"],
                ],],
                ev(Ev::Click, |_| {
                    Urls::library_abs()
                        .add_hash_path_part(MUSIC_LIBRARY_PL_STATIC)
                        .go_and_load()
                }),
            ],
            li![
                IF!(page_name == "Settings" => C!["is-active"]),
                a![span![
                    C!["icon", "is-small"],
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "tune"],
                ]],
                ev(Ev::Click, |_| { Urls::settings_abs().go_and_load() }),
            ],
        ]
    ]
}

fn view_music_lib_tabs(page: &Page) -> Node<Msg> {
    let page_name: &str = page.into();
    div![
        C!["tabs", "is-boxed", "is-centered", "is-toggle", "pt-3"],
        ul![
            li![
                C!["has-background-dark-transparent"],
                IF!(page_name == "MusicLibraryStaticPlaylist" => C!["is-active"]),
                a![
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_PL_STATIC)},
                    span!("Playlists")
                ]
            ],
            li![
                C!["has-background-dark-transparent"],
                IF!(page_name == "MusicLibraryFiles" => C!["is-active"]),
                a![
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_FILES)},
                    span!("Files"),
                ],
            ],
            li![
                C!["has-background-dark-transparent"],
                IF!(page_name == "MusicLibraryRadio" => C!["is-active"]),
                a![
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_RADIO)},
                    span!("Radio")
                ]
            ],
            li![
                C!["has-background-dark-transparent"],
                IF!(page_name == "MusicLibraryArtists" => C!["is-active"]),
                a![
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_ARTISTS)},
                    span!("Artists"),
                ],
            ],
            li![
                C!["has-background-dark-transparent"],
                IF!(page_name == "MusicLibraryDynamicPlaylist" => C!["is-active"]),
                a![attrs! {At::Href => ""}, span!("Discover")]
            ],
        ],
    ]
}

pub fn view_spinner_modal<Ms>(active: bool) -> Node<Ms> {
    // spinner
    div![
        C!["modal", IF!(active => "is-active")],
        div![C!["modal-background"]],
        div![
            C!["modal-content"],
            div![
                C!("sk-fading-circle"),
                div![C!["sk-circle1 sk-circle"]],
                div![C!["sk-circle2 sk-circle"]],
                div![C!["sk-circle3 sk-circle"]],
                div![C!["sk-circle4 sk-circle"]],
                div![C!["sk-circle5 sk-circle"]],
                div![C!["sk-circle6 sk-circle"]],
                div![C!["sk-circle7 sk-circle"]],
                div![C!["sk-circle8 sk-circle"]],
                div![C!["sk-circle9 sk-circle"]],
                div![C!["sk-circle10 sk-circle"]],
                div![C!["sk-circle11 sk-circle"]],
                div![C!["sk-circle12 sk-circle"]],
            ]
        ]
    ]
}

// ------ ------
//     Start
// ------ ------

#[wasm_bindgen(start)]
pub fn start() {
    App::start("app", init, update, view);
}

// ------ ------
//    Extern
// ------ ------

#[wasm_bindgen]
extern "C" {
    pub fn scrollToId(id: &str);
    pub fn attachCarousel(id: &str);
}

fn create_websocket(orders: &impl Orders<Msg>) -> Result<EventClient, WebSocketError> {
    let current = seed::browser::util::window().location();
    let protocol = current.protocol().expect("Can't get protocol");
    let host = current.host().expect("Cant get host");
    let ws_url = format!(
        "{}//{}/{}",
        (if protocol == "https:" { "wss:" } else { "ws:" }),
        host,
        "api/ws"
    );
    let msg_sender = orders.msg_sender();

    let mut client = EventClient::new(&ws_url)?;

    client.set_on_error(Some(Box::new(|error| {
        error!("WS: {:#?}", error);
    })));

    let send = msg_sender.clone();
    client.set_on_connection(Some(Box::new(move |client: &EventClient| {
        log!(format!("{:#?}", client.status));
        let msg = match *client.status.borrow() {
            ConnectionStatus::Connecting => {
                log!("Connecting...");
                None
            }
            ConnectionStatus::Connected => Some(Msg::WebSocketOpened),
            ConnectionStatus::Error => Some(Msg::WebSocketFailed),
            ConnectionStatus::Disconnected => {
                log!("Disconnected");
                None
            }
        };
        send(msg);
    })));

    let send = msg_sender.clone();
    client.set_on_close(Some(Box::new(move |ev| {
        log!("WS: Connection closed");
        send(Some(Msg::WebSocketClosed(ev)));
    })));

    let send = msg_sender.clone();
    client.set_on_message(Some(Box::new(move |_: &EventClient, msg: wasm_sockets::Message| {
        decode_message(msg, &Rc::clone(&send))
    })));
    Ok(client)
}

fn decode_message(message: Message, msg_sender: &Rc<dyn Fn(Option<Msg>)>) {
    match message {
        Message::Text(txt) => {
            msg_sender(Some(Msg::WebSocketMessageReceived(txt)));
        }
        Message::Binary(_) => {}
    }
}

#[allow(clippy::future_not_send)]
async fn update_album_cover(track: Song) -> Msg {
    if let Some(image_id) = track.image_id {
        return Msg::AlbumImageUpdated(Image {
            size: "mega".to_string(),
            text: format!("/artwork/{}", image_id),
        });
    };
    if track.album.is_some() && track.artist.is_some() {
        let ai = get_album_image_from_lastfm_api(track.album.unwrap(), track.artist.unwrap()).await;
        ai.map_or_else(|| Msg::Ignore, Msg::AlbumImageUpdated)
    } else {
        Msg::Ignore
    }
}

#[allow(clippy::future_not_send)]
async fn get_album_image_from_lastfm_api(album: String, artist: String) -> Option<Image> {
    let current = seed::browser::util::window().location();
    let protocol = current.protocol().map_or("http:".to_owned(), |f| f);
    let response = Request::get(format!("{protocol}//ws.audioscrobbler.com/2.0/?api_key=3b3df6c5dd3ad07222adc8dd3ccd8cdc&format=json&method=album.getinfo&album={album}&artist={artist}").as_str()).send().await;
    if let Ok(response) = response {
        let info = response.json::<AlbumInfo>().await;
        if let Ok(info) = info {
            info.album
                .image
                .into_iter()
                .find(|i| i.size == "mega" && !i.text.is_empty())
        } else {
            log!(format!("Failed to get album info {:?}", info));
            None
        }
    } else if let Err(e) = response {
        log!(format!("Error getting album info from last.fm {:?}", e));
        None
    } else {
        None
    }
}

fn get_background_image(model: &PlayerModel) -> Option<String> {
    if let Some(ps) = model.current_song.as_ref() {
        if let Some(image_id) = ps.image_id.as_ref() {
            return Some(format!("/artwork/{}", image_id));
        };
        return ps.image_url.clone();
    }
    None
}

#[cfg(test)]
mod test {

    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::Urls;

    #[wasm_bindgen_test]
    async fn test_get_search_term() {
        let url = Urls::library_abs().add_hash_path_part("artist?search=abc");
        assert_eq!(Urls::get_search_term(&url).unwrap(), "abc");
    }
}
