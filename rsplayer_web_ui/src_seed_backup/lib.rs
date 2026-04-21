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
mod lyrics;

const SETTINGS: &str = "settings";

const QUEUE: &str = "queue";
const FIRST_SETUP: &str = "setup";
const PLAYER: &str = "player";
const MUSIC_LIBRARY: &str = "library";
const MUSIC_LIBRARY_FILES: &str = "files";
const MUSIC_LIBRARY_ARTISTS: &str = "artists";
const MUSIC_LIBRARY_RADIO: &str = "radio";
const MUSIC_LIBRARY_PL_STATIC: &str = "playlists";
const MUSIC_LIBRARY_STATS: &str = "stats";

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
    lyrics_modal_open: bool,
    lyrics_loading: bool,
    lyrics: Option<lyrics::LrcLibResponse>,
    parsed_lyrics: Option<Vec<lyrics::LyricLine>>,
    ring_buffer_size_ms: usize,
    last_active_lyrics_idx: Option<usize>,
    pre_mute_volume: Option<u8>,
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
    visualizer_type: vumeter::VisualizerType,
    /// Name of the currently active theme (e.g. "dark", "light", "solarized", "high-contrast").
    current_theme: String,
    /// Whether the library sub-nav dropdown is expanded.
    library_nav_open: bool,
    local_browser_playback: bool,
    /// Whether to show the welcome modal for first-time users.
    show_welcome_modal: bool,
    /// Whether to show the keyboard shortcuts help modal.
    show_keyboard_shortcuts: bool,
    /// Whether this is the first visit (persists after modal dismiss).
    is_first_visit: bool,
    /// Global settings (fetched on init, used to check if playback device is configured)
    global_settings: Option<api_models::settings::Settings>,
    /// Set to true when we push back in history to cancel a navigation from dirty settings.
    /// The resulting synthetic UrlChanged is skipped to avoid re-initializing the settings page.
    skip_next_url_change: bool,
    demo_mode: bool,
}

/// Tabs for keyboard navigation
#[derive(Debug, Clone, Copy)]
pub enum Tab {
    Player,
    Queue,
    Library,
    LibraryFiles,
    LibraryArtists,
    LibraryPlaylists,
    LibraryRadio,
    LibraryStats,
    Settings,
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
    SettingsFetchedGlobal(api_models::settings::Settings),
    Settings(page::settings::Msg),
    StartErrorReceived(String),
    Queue(page::queue::Msg),
    MusicLibraryStaticPlaylist(page::music_library_static_playlist::Msg),
    MusicLibraryFiles(page::music_library_files::Msg),
    MusicLibraryArtists(page::music_library_artists::Msg),
    MusicLibraryRadio(page::music_library_radio::Msg),
    LibraryStats(page::library_stats::Msg),
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
    ToggleVisualizer,
    WindowResized,

    LikeMediaItemClick(MetadataCommand),
    /// Cycle to the next theme (calls JS cycleTheme()).
    CycleTheme,
    /// Apply a specific theme by name (from the settings picker).
    ChangeTheme(String),
    /// Toggle the library sub-nav dropdown open/closed.
    ToggleLibraryNav,

    ToggleLyricsModal,
    FetchLyrics,
    LyricsFetched(Option<lyrics::LrcLibResponse>),

    BrowserAudioTimeUpdate(f64, f64),
    BrowserAudioEnded,
    BrowserAudioPaused,
    BrowserAudioPlaying,
    MediaNextTrack,
    MediaPrevTrack,
    
    /// Global keyboard shortcuts
    TogglePlayPause,
    ToggleMute,
    
    /// Navigate to specific tab
    NavigateToTab(Tab),
    
    /// Show/hide welcome modal for first-time users.
    ToggleWelcomeModal,
    /// Dismiss welcome modal permanently.
    DismissWelcomeModal,
    /// Show/hide keyboard shortcuts help modal.
    ToggleKeyboardShortcutsHelp,
    /// Close all open modals.
    CloseModals,
    /// Focus on search input field.
    FocusSearch,
    /// Toggle like on current track.
    ToggleLike,
    /// Toggle lyrics modal.
    ToggleLyrics,
    /// Cycle shuffle/repeat mode.
    CycleShuffleMode,
    /// Seek backward 10 seconds.
    SeekBackward,
    /// Seek forward 10 seconds.
    SeekForward,
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
    LibraryStats(page::library_stats::Model),
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
                MUSIC_LIBRARY_STATS => Self::LibraryStats(page::library_stats::init(
                    url,
                    &mut orders.proxy(Msg::LibraryStats),
                )),
                _ => Self::NotFound,
            },
            PLAYER | "" => Self::Player,
            _ => Self::NotFound,
        }
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
    
    // Global keyboard shortcuts - works on all pages
    orders.stream(streams::window_event(Ev::KeyDown, |event| {
        use api_models::common::UserCommand::Player;
        use api_models::common::{PlayerCommand, SystemCommand};
        
        // Clone what we need before converting event
        let target = event.target();
        let is_input = target.as_ref().is_some_and(|t| {
            let el = t.dyn_ref::<web_sys::Element>();
            el.is_some_and(|e| {
                let tag = e.tag_name();
                tag == "INPUT" || tag == "TEXTAREA"
            })
        });
        
        if is_input {
            return Msg::Ignore;
        }
        
        // Now convert and use the keyboard event
        let keyboard_event: web_sys::KeyboardEvent = event.unchecked_into();
        let key = keyboard_event.key();
        let repeat = keyboard_event.repeat();
        let shift_key = keyboard_event.shift_key();
        
        match key.as_str() {
            "?" => {
                keyboard_event.prevent_default();
                Msg::ToggleKeyboardShortcutsHelp
            }
            "/" => {
                // Focus search input
                keyboard_event.prevent_default();
                Msg::FocusSearch
            }
            "Escape" => {
                Msg::CloseModals
            }
            " " => {
                if !repeat {
                    keyboard_event.prevent_default();
                    Msg::TogglePlayPause
                } else {
                    Msg::Ignore
                }
            }
            "ArrowLeft" => {
                keyboard_event.prevent_default();
                if shift_key {
                    Msg::SeekBackward
                } else {
                    Msg::SendUserCommand(Player(PlayerCommand::Prev))
                }
            }
            "ArrowRight" => {
                keyboard_event.prevent_default();
                if shift_key {
                    Msg::SeekForward
                } else {
                    Msg::SendUserCommand(Player(PlayerCommand::Next))
                }
            }
            "ArrowUp" => {
                keyboard_event.prevent_default();
                Msg::SendSystemCommand(SystemCommand::VolUp)
            }
            "ArrowDown" => {
                keyboard_event.prevent_default();
                Msg::SendSystemCommand(SystemCommand::VolDown)
            }
            "m" | "M" => {
                keyboard_event.prevent_default();
                Msg::ToggleMute
            }
            // Player controls
            "l" | "L" => {
                // Like/Unlike current track
                keyboard_event.prevent_default();
                Msg::ToggleLike
            }
            "v" | "V" => {
                keyboard_event.prevent_default();
                Msg::ToggleVisualizer
            }
            "y" | "Y" => {
                // Toggle lyrics modal
                keyboard_event.prevent_default();
                Msg::ToggleLyrics
            }
            "s" | "S" => {
                // Cycle shuffle/repeat mode
                keyboard_event.prevent_default();
                Msg::CycleShuffleMode
            }
            // Tab navigation
            "1" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::Player)
            }
            "2" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::Queue)
            }
            "3" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::Library)
            }
            "4" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::Settings)
            }
            // Library sub-tabs
            "f" | "F" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::LibraryFiles)
            }
            "a" | "A" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::LibraryArtists)
            }
            "p" | "P" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::LibraryPlaylists)
            }
            "r" | "R" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::LibraryRadio)
            }
            "t" | "T" => {
                keyboard_event.prevent_default();
                Msg::NavigateToTab(Tab::LibraryStats)
            }
            _ => Msg::Ignore,
        }
    }));

    if matches!(page, Page::Player) {
        orders.after_next_render(|_| Some(Msg::InitVUMeter));
    }

    orders.perform_cmd(async {
        let response = Request::get("/api/settings").send().await;
        if let Ok(response) = response {
            if response.ok() {
                if let Ok(sett) = response.json::<api_models::settings::Settings>().await {
                    return Msg::SettingsFetchedGlobal(sett);
                }
            }
        }
        Msg::Ignore
    });

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
    // Read whichever theme JS already applied (from localStorage / system pref).
    let current_theme = getTheme();
    // Check if this is the first visit.
    let show_welcome_modal = isFirstVisit();

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
            lyrics_modal_open: false,
            lyrics_loading: false,
            lyrics: None,
            parsed_lyrics: None,
            ring_buffer_size_ms: 200, // Default value
            last_active_lyrics_idx: None,
            pre_mute_volume: None,
        },
        metadata_scan_info: None,
        notification: None,
        vumeter: None,
        visualizer_type: load_visualizer_type(),
        current_theme,
        library_nav_open: false,
        local_browser_playback: false,
        show_welcome_modal,
        show_keyboard_shortcuts: false,
        is_first_visit: show_welcome_modal, // Same as initial welcome modal state
        global_settings: None,
        skip_next_url_change: false,
        demo_mode: false,
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
    fn library_files_abs() -> Url {
        Url::new()
            .add_hash_path_part(MUSIC_LIBRARY)
            .add_hash_path_part(MUSIC_LIBRARY_FILES)
    }
    fn library_artists_abs() -> Url {
        Url::new()
            .add_hash_path_part(MUSIC_LIBRARY)
            .add_hash_path_part(MUSIC_LIBRARY_ARTISTS)
    }
    fn library_playlists_abs() -> Url {
        Url::new()
            .add_hash_path_part(MUSIC_LIBRARY)
            .add_hash_path_part(MUSIC_LIBRARY_PL_STATIC)
    }
    fn library_radio_abs() -> Url {
        Url::new()
            .add_hash_path_part(MUSIC_LIBRARY)
            .add_hash_path_part(MUSIC_LIBRARY_RADIO)
    }
    fn library_stats_abs() -> Url {
        Url::new()
            .add_hash_path_part(MUSIC_LIBRARY)
            .add_hash_path_part(MUSIC_LIBRARY_STATS)
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
        Msg::SettingsFetchedGlobal(sett) => {
            model.local_browser_playback = sett.local_browser_playback;
            model.demo_mode = sett.demo_mode;
            // In browser mode there's no ring buffer — no latency offset needed
            model.player_model.ring_buffer_size_ms = if sett.local_browser_playback {
                0
            } else {
                sett.rs_player_settings.ring_buffer_size_ms
            };
            if sett.local_browser_playback {
                setupMediaSessionHandlers();
                orders.stream(streams::window_event(Ev::from("media-nexttrack"), |_| Msg::MediaNextTrack));
                orders.stream(streams::window_event(Ev::from("media-previoustrack"), |_| Msg::MediaPrevTrack));
            }
            // Check if playback device is configured
            let playback_configured = sett.local_browser_playback 
                || (!sett.alsa_settings.output_device.card_id.is_empty() 
                    && !sett.alsa_settings.output_device.name.is_empty());
            log!(format!("SettingsFetchedGlobal: is_first_visit={}, playback_configured={}, local_browser_playback={}, card_id={}, pcm_name={}", 
                model.is_first_visit,
                playback_configured,
                sett.local_browser_playback,
                sett.alsa_settings.output_device.card_id,
                sett.alsa_settings.output_device.name
            ));
            // Store settings for later use
            model.global_settings = Some(sett);
            // Note: Welcome modal already shows "Required Setup" notice with "Go to Settings" button
            // User will navigate manually via the modal button (avoids RefCell borrow conflict)
        }
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
            if let Page::LibraryStats(_) = &model.page {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(
                    MetadataCommand::QueryLibraryStats,
                )));
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
            // Skip the synthetic URL change triggered by history.go(-1) when we cancel navigation.
            if model.skip_next_url_change {
                model.skip_next_url_change = false;
                return;
            }
            // Warn before navigating away from settings with unapplied DSP changes.
            if let Page::Settings(ref sett_model) = model.page {
                if sett_model.has_unsaved_changes() {
                    let confirmed = web_sys::window()
                        .and_then(|w| w.confirm_with_message("You have changes that require a player restart to take effect. Leave without restarting?").ok())
                        .unwrap_or(true);
                    if !confirmed {
                        model.skip_next_url_change = true;
                        if let Some(w) = web_sys::window() {
                            let _ = w.history().map(|h| h.go_with_delta(-1));
                        }
                        return;
                    }
                }
            }
            model.page = Page::new(url, orders);
            // Close library dropdown when navigating away from library section.
            if !matches!(
                model.page,
                Page::MusicLibraryFiles(_)
                    | Page::MusicLibraryStaticPlaylist(_)
                    | Page::MusicLibraryArtists(_)
                    | Page::MusicLibraryRadio(_)
                    | Page::LibraryStats(_)
            ) {
                model.library_nav_open = false;
            }
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
            if model.local_browser_playback {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Some(audio_el) = document.get_element_by_id("local-audio-player") {
                            let audio: web_sys::HtmlAudioElement = audio_el.unchecked_into();
                            audio.set_current_time(pos as f64);
                        }
                    }
                }
            }
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
                if let Some(meter) = vumeter::VUMeter::with_type("vumeter", model.visualizer_type) {
                    model.vumeter = Some(meter);
                }
            } else {
                model.vumeter = None;
            }
        }
        Msg::ToggleVisualizer => {
            model.visualizer_type = match model.visualizer_type {
                vumeter::VisualizerType::None => vumeter::VisualizerType::NeonBar,
                vumeter::VisualizerType::NeonBar => vumeter::VisualizerType::Spectrum,
                vumeter::VisualizerType::Spectrum => vumeter::VisualizerType::Wave,
                vumeter::VisualizerType::Wave => vumeter::VisualizerType::Circular,
                vumeter::VisualizerType::Circular => vumeter::VisualizerType::Lissajous,
                vumeter::VisualizerType::Lissajous => vumeter::VisualizerType::Particles,
                vumeter::VisualizerType::Particles => vumeter::VisualizerType::Mirror,
                vumeter::VisualizerType::Mirror => vumeter::VisualizerType::Starfield,
                vumeter::VisualizerType::Starfield => vumeter::VisualizerType::Dna,
                vumeter::VisualizerType::Dna => vumeter::VisualizerType::Plasma,
                vumeter::VisualizerType::Plasma => vumeter::VisualizerType::Tunnel,
                vumeter::VisualizerType::Tunnel => vumeter::VisualizerType::Bounce,
                vumeter::VisualizerType::Bounce => vumeter::VisualizerType::None,
            };
            save_visualizer_type(model.visualizer_type);
            if let Some(meter) = vumeter::VUMeter::with_type("vumeter", model.visualizer_type) {
                model.vumeter = Some(meter);
            }
        }
        Msg::WindowResized => {
            if let Some(meter) = &mut model.vumeter {
                meter.resize();
            }
        }
        Msg::SetVolume(volstr) => {
            if model.local_browser_playback {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Some(audio_el) = document.get_element_by_id("local-audio-player") {
                            let audio: web_sys::HtmlAudioElement = audio_el.unchecked_into();
                            let vol = u8::from_str(volstr.as_str()).unwrap_or(model.player_model.volume_state.current);
                            audio.set_volume(vol as f64 / model.player_model.volume_state.max as f64);
                        }
                    }
                }
            }
            log!("New vol string {}", &volstr);
            model.player_model.stop_updates = true;
            let vol = u8::from_str(volstr.as_str()).unwrap_or(model.player_model.volume_state.current);
            model.player_model.volume_state.current = vol;
            orders.send_msg(Msg::SendSystemCommand(SystemCommand::SetVol(vol)));
            orders.perform_cmd(cmds::timeout(100, || Msg::ResumeUpdates));
        }
        Msg::SetVolumeInput(volstr) => {
            if model.local_browser_playback {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Some(audio_el) = document.get_element_by_id("local-audio-player") {
                            let audio: web_sys::HtmlAudioElement = audio_el.unchecked_into();
                            let vol = u8::from_str(volstr.as_str()).unwrap_or(model.player_model.volume_state.current);
                            audio.set_volume(vol as f64 / model.player_model.volume_state.max as f64);
                        }
                    }
                }
            }
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
                    if model.local_browser_playback {
                        let title = song.title.as_deref().unwrap_or("");
                        let artist = song.artist.as_deref().unwrap_or("");
                        let album = song.album.as_deref().unwrap_or("");
                        let artwork = get_background_image(&model.player_model)
                            .unwrap_or_default();
                        updateMediaSessionMetadata(title, artist, album, &artwork);
                    }
                    model.player_model.lyrics = None;
                    model.player_model.parsed_lyrics = None;
                    model.player_model.last_active_lyrics_idx = None;
                    if model.player_model.lyrics_modal_open {
                        orders.send_msg(Msg::FetchLyrics);
                    }
                }
                StateChangeEvent::VolumeChangeEvent(vol) => {
                    if model.player_model.volume_state.current != vol.current {
                        model.player_model.volume_state = *vol;
                        if model.local_browser_playback {
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(audio_el) = document.get_element_by_id("local-audio-player") {
                                        let audio: web_sys::HtmlAudioElement = audio_el.unchecked_into();
                                        audio.set_volume(vol.current as f64 / vol.max as f64);
                                    }
                                }
                            }
                        }
                    }
                }                StateChangeEvent::PlayerInfoEvent(pi) => {
                    model.player_model.player_info = Some(pi.clone());
                }
                StateChangeEvent::PlaybackModeChangedEvent(mode) => {
                    model.player_model.playback_mode = *mode;
                }
                StateChangeEvent::PlaybackStateEvent(ps) => {
                    model.player_model.player_state = ps.clone();
                    if model.local_browser_playback {
                        if let Some(window) = web_sys::window() {
                            if let Some(document) = window.document() {
                                if let Some(audio_el) = document.get_element_by_id("local-audio-player") {
                                    let audio: web_sys::HtmlAudioElement = audio_el.unchecked_into();
                                    match ps {
                                        PlayerState::PLAYING => { let _ = audio.play(); },
                                        PlayerState::PAUSED | PlayerState::STOPPED => { let _ = audio.pause(); },
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
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
            } else if let Page::LibraryStats(model) = &mut model.page {
                page::library_stats::update(
                    page::library_stats::Msg::StatusChangeEventReceived(chg_ev),
                    model,
                    &mut orders.proxy(Msg::LibraryStats),
                );
            } else if let Page::Settings(sett_model) = &mut model.page {
                match chg_ev {
                    StateChangeEvent::MountStatusEvent(statuses) => {
                        page::settings::update(
                            page::settings::Msg::MountStatusReceived(statuses),
                            sett_model,
                            &mut orders.proxy(Msg::Settings),
                        );
                    }
                    StateChangeEvent::MusicDirStatusEvent(statuses) => {
                        page::settings::update(
                            page::settings::Msg::MusicDirStatusReceived(statuses),
                            sett_model,
                            &mut orders.proxy(Msg::Settings),
                        );
                    }
                    StateChangeEvent::ExternalMountsEvent(mounts) => {
                        page::settings::update(
                            page::settings::Msg::ExternalMountsReceived(mounts),
                            sett_model,
                            &mut orders.proxy(Msg::Settings),
                        );
                    }
                    _ => {}
                }
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
                if let page::settings::Msg::SelectTheme(theme) = &msg {
                    orders.send_msg(Msg::ChangeTheme(theme.clone()));
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
        Msg::LibraryStats(msg) => {
            if let Page::LibraryStats(stats_model) = &mut model.page {
                if let page::library_stats::Msg::SendUserCommand(cmd) = &msg {
                    _ = model.web_socket.send_string(&serde_json::to_string(cmd).unwrap());
                }
                page::library_stats::update(msg, stats_model, &mut orders.proxy(Msg::LibraryStats));
            }
        }

        Msg::WebSocketMessageReceived(message) => {
            let msg = serde_json::from_str::<StateChangeEvent>(&message)
                .unwrap_or_else(|_| panic!("Failed to decode WebSocket text message: {message}"));
            if let StateChangeEvent::SongTimeEvent(st) = msg {
                // In browser playback mode, the <audio> element drives progress
                // via BrowserAudioTimeUpdate — ignore backend time events
                if model.local_browser_playback {
                    return;
                }
                if model.player_model.stop_updates {
                    log!("Updates are stopped");
                    return;
                }
                model.player_model.progress = st;
                model.player_model.player_state = PlayerState::PLAYING;

                // Handle lyrics synchronization and scrolling
                let current_time = model.player_model.progress.current_time.as_secs_f64() - (model.player_model.ring_buffer_size_ms as f64 / 1000.0);
                let active_idx = model.player_model.parsed_lyrics.as_ref().and_then(|lines| {
                    lines.iter().rposition(|line| line.time_secs <= current_time)
                });

                if active_idx != model.player_model.last_active_lyrics_idx {
                    model.player_model.last_active_lyrics_idx = active_idx;
                    if model.player_model.lyrics_modal_open && active_idx.is_some() {
                        orders.after_next_render(|_| scrollToId("lyric-active"));
                    }
                }
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
        Msg::CycleTheme => {
            model.current_theme = cycleTheme();
        }
        Msg::ChangeTheme(theme) => {
            model.current_theme = applyTheme(&theme);
        }
        Msg::ToggleLibraryNav => {
            model.library_nav_open = !model.library_nav_open;
        }

        Msg::ToggleLyricsModal => {
            model.player_model.lyrics_modal_open = !model.player_model.lyrics_modal_open;
            if model.player_model.lyrics_modal_open && model.player_model.lyrics.is_none() {
                orders.send_msg(Msg::FetchLyrics);
            }
        }

        Msg::FetchLyrics => {
            if let Some(song) = model.player_model.current_song.clone() {
                model.player_model.lyrics_loading = true;
                orders.perform_cmd(fetch_lyrics_task(song));
            }
        }

        Msg::LyricsFetched(lyrics) => {
            model.player_model.lyrics_loading = false;
            model.player_model.lyrics = lyrics.clone();
            if let Some(lrc) = lyrics.and_then(|l| l.synced_lyrics) {
                model.player_model.parsed_lyrics = Some(lyrics::parse_lrc(&lrc));
            } else {
                model.player_model.parsed_lyrics = None;
            }
        }

        Msg::BrowserAudioTimeUpdate(current, duration) => {
            if model.player_model.stop_updates || !model.local_browser_playback {
                return;
            }
            // Guard against Infinity/NaN (radio streams have infinite duration)
            if current.is_finite() {
                model.player_model.progress.current_time = std::time::Duration::from_secs_f64(current);
            }
            if duration.is_finite() {
                model.player_model.progress.total_time = std::time::Duration::from_secs_f64(duration);
            }
            model.player_model.player_state = PlayerState::PLAYING;

            // Lyrics synchronization (ring_buffer_size_ms is 0 in browser mode)
            let latency_offset = model.player_model.ring_buffer_size_ms as f64 / 1000.0;
            let current_time = current - latency_offset;
            let active_idx = model.player_model.parsed_lyrics.as_ref().and_then(|lines| {
                lines.iter().rposition(|line| line.time_secs <= current_time)
            });
            if active_idx != model.player_model.last_active_lyrics_idx {
                model.player_model.last_active_lyrics_idx = active_idx;
                if model.player_model.lyrics_modal_open && active_idx.is_some() {
                    orders.after_next_render(|_| scrollToId("lyric-active"));
                }
            }
        }

        Msg::BrowserAudioEnded => {
            if model.local_browser_playback {
                orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::Next)));
            }
        }

        Msg::BrowserAudioPaused => {
            if model.local_browser_playback {
                model.player_model.player_state = PlayerState::PAUSED;
            }
        }

        Msg::BrowserAudioPlaying => {
            if model.local_browser_playback {
                model.player_model.player_state = PlayerState::PLAYING;
            }
        }

        Msg::MediaNextTrack => {
            if model.local_browser_playback {
                orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::Next)));
            }
        }

        Msg::MediaPrevTrack => {
            if model.local_browser_playback {
                orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::Prev)));
            }
        }
        
        Msg::TogglePlayPause => {
            let cmd = if model.player_model.player_state == PlayerState::PLAYING {
                Player(PlayerCommand::Pause)
            } else {
                Player(PlayerCommand::Play)
            };
            orders.send_msg(Msg::SendUserCommand(cmd));
        }
        
        Msg::ToggleMute => {
            let is_muted = model.player_model.volume_state.current == 0;
            let new_vol = if is_muted {
                model.player_model.pre_mute_volume.take().unwrap_or(model.player_model.volume_state.max / 2)
            } else {
                model.player_model.pre_mute_volume = Some(model.player_model.volume_state.current);
                0
            };
            orders.send_msg(Msg::SendSystemCommand(SystemCommand::SetVol(new_vol)));
        }
        
        Msg::ToggleLike => {
            // Toggle like on current track
            if let Some(song) = &model.player_model.current_song {
                let id = song.file.clone();
                let is_liked = song.statistics.as_ref().is_some_and(|stat| stat.liked_count > 0);
                let cmd = if is_liked {
                    MetadataCommand::DislikeMediaItem(id)
                } else {
                    MetadataCommand::LikeMediaItem(id)
                };
                orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(cmd)));
            }
        }
        
        Msg::ToggleLyrics => {
            model.player_model.lyrics_modal_open = !model.player_model.lyrics_modal_open;
            if model.player_model.lyrics_modal_open && model.player_model.lyrics.is_none() {
                orders.send_msg(Msg::FetchLyrics);
            }
        }
        
        Msg::CycleShuffleMode => {
            orders.send_msg(Msg::SendUserCommand(Player(PlayerCommand::CyclePlaybackMode)));
        }
        
        Msg::SeekBackward => {
            // Seek back 10 seconds
            let current = model.player_model.progress.current_time.as_secs();
            let new_pos = current.saturating_sub(10) as u16;
            orders.send_msg(Msg::SeekTrackPosition(new_pos));
        }
        
        Msg::SeekForward => {
            // Seek forward 10 seconds
            let current = model.player_model.progress.current_time.as_secs();
            let total = model.player_model.progress.total_time.as_secs();
            let new_pos = (current + 10).min(total) as u16;
            orders.send_msg(Msg::SeekTrackPosition(new_pos));
        }
        
        Msg::NavigateToTab(tab) => {
            let url = match tab {
                Tab::Player => Urls::player_abs(),
                Tab::Queue => Urls::queue_abs(),
                Tab::Library => Urls::library_files_abs(),
                Tab::LibraryFiles => Urls::library_files_abs(),
                Tab::LibraryArtists => Urls::library_artists_abs(),
                Tab::LibraryPlaylists => Urls::library_playlists_abs(),
                Tab::LibraryRadio => Urls::library_radio_abs(),
                Tab::LibraryStats => Urls::library_stats_abs(),
                Tab::Settings => Urls::settings_abs(),
            };
            let hash = url.hash_path().join("/");
            // Use JS function that defers navigation with setTimeout
            navigateToHash(&hash);
        }
        
        Msg::ToggleWelcomeModal => {
            model.show_welcome_modal = !model.show_welcome_modal;
        }
        
        Msg::DismissWelcomeModal => {
            model.show_welcome_modal = false;
            markVisited();
        }
        
        Msg::ToggleKeyboardShortcutsHelp => {
            model.show_keyboard_shortcuts = !model.show_keyboard_shortcuts;
        }
        
        Msg::CloseModals => {
            model.show_keyboard_shortcuts = false;
            model.show_welcome_modal = false;
            model.player_model.lyrics_modal_open = false;
        }
        
        Msg::FocusSearch => {
            focusSearchInput();
        }

        Msg::Ignore => {}
        _ => {}
    }
}

// ------ ------
//     View
// ------ ------

fn view(model: &Model) -> impl IntoNodes<Msg> {
    let (bg_image, bg_color) = if let Some(img) = get_background_image(&model.player_model) {
        (format!("url({})", img), String::new())
    } else {
        (String::new(), "var(--background)".to_string())
    };

    nodes![
        div![
            C!["container"],
            style! {
                St::MinHeight => "100vh",
                St::Position => "relative",
                St::BackgroundColor => bg_color,
            },
            // Sticky background layer constrained to container width
            div![
                style! {
                    St::Position => "sticky",
                    St::Top => "0",
                    St::Height => "100vh",
                    St::Width => "100%",
                    St::ZIndex => 0,
                    St::BackgroundImage => bg_image,
                    St::BackgroundRepeat => "no-repeat",
                    St::BackgroundSize => "cover",
                    St::BackgroundPosition => "center",
                    St::PointerEvents => "none",
                },
            ],
            // Content layer - pulled up to overlap sticky background
            div![
                style! {
                    St::Position => "relative",
                    St::MarginTop => "-100vh",
                    St::MinHeight => "100vh",
                    St::ZIndex => 1,
                },
                view_demo_mode_banner(model),
                view_navigation_tabs(&model.page, model.library_nav_open),
                view_startup_error(model.startup_error.as_ref()),
                view_reconnect_notification(model),
                view_metadata_scan_notification(model),
                view_local_browser_playback(model),
                view_content(model, &model.base_url),
                view_player_footer(&model.page, &model.player_model),
                view_notification(model),
            ]
        ],
        // Welcome modal (outside container to overlay everything)
        IF!(model.show_welcome_modal => view_welcome_modal()),
        // Keyboard shortcuts help modal
        IF!(model.show_keyboard_shortcuts => view_keyboard_shortcuts_help()),
        // Help button - only show on first visit
        IF!(model.is_first_visit => button![
            C!["help-tooltip-trigger"],
            attrs! { At::Title => "Show help / Getting started" },
            i![C!["material-icons"], "help_outline"],
            ev(Ev::Click, |_| Msg::ToggleWelcomeModal)
        ]),
        // Keyboard shortcut hint - hidden on mobile via CSS
        div![
            C!["keyboard-hint"],
            attrs! { At::Title => "Press ? for keyboard shortcuts" },
            "?",
            ev(Ev::Click, |_| Msg::ToggleKeyboardShortcutsHelp),
        ],
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

fn view_demo_mode_banner(model: &Model) -> Node<Msg> {
    if !model.demo_mode {
        return empty!();
    }
    div![
        C!["notification", "is-warning", "is-light"],
        style! {
            St::Margin => "0",
            St::BorderRadius => "0",
            St::TextAlign => "center",
            St::Padding => "0.5rem 1rem",
        },
        "Demo mode \u{2014} some features might not be available."
    ]
}

fn view_notification(model: &Model) -> Node<Msg> {
    div![
        style! {
            St::ZIndex => 1000,
            St::Top => "20px",
            St::Right => "20px",
            St::Position => "fixed",
            St::MaxWidth => "300px",
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

fn view_welcome_modal() -> Node<Msg> {
    div![
        C!["modal", "is-active", "welcome-modal"],
        div![
            C!["modal-background"],
            ev(Ev::Click, |_| Msg::ToggleWelcomeModal)
        ],
        div![
            C!["modal-content"],
            style! {
                St::BackgroundColor => "var(--ui-elements)",
                St::Padding => "2rem",
                St::BorderRadius => "12px",
                St::MaxWidth => "550px",
                St::BoxShadow => "0 20px 60px rgba(0, 0, 0, 0.5)",
            },
            // Header
            div![
                C!["welcome-modal__header"],
                i![C!["material-icons", "welcome-modal__icon"], "music_note"],
                h2![C!["welcome-modal__title"], "Welcome to RSPlayer"],
                p![C!["welcome-modal__subtitle"], "Your personal music streaming server"],
            ],
            // Required setup notice
            div![
                style! {
                    St::BackgroundColor => "rgba(255, 180, 0, 0.15)",
                    St::Border => "1px solid rgba(255, 180, 0, 0.4)",
                    St::BorderRadius => "8px",
                    St::Padding => "1rem",
                    St::MarginBottom => "1.5rem",
                },
                div![
                    style! { St::Display => "flex", St::AlignItems => "center", St::Gap => "0.5rem", St::MarginBottom => "0.5rem" },
                    i![C!["material-icons"], style! { St::Color => "#ffb400", St::FontSize => "20px" }, "warning"],
                    span![style! { St::FontWeight => "600", St::Color => "#ffb400" }, "Required Setup"],
                ],
                p![
                    style! { St::FontSize => "0.9rem", St::Color => "var(--secondary-text)", St::Margin => "0" },
                    "Before you can play music, you need to configure the audio interface in Settings."
                ],
            ],
            // Steps
            div![
                C!["welcome-modal__steps"],
                div![
                    C!["welcome-modal__step"],
                    div![C!["welcome-modal__step-number"], "1"],
                    div![
                        C!["welcome-modal__step-content"],
                        div![
                            style! { St::Display => "flex", St::AlignItems => "center", St::Gap => "0.5rem" },
                            div![C!["welcome-modal__step-title"], "Audio Interface"],
                            span![
                                style! {
                                    St::BackgroundColor => "#ffb400",
                                    St::Color => "#000",
                                    St::FontSize => "0.7rem",
                                    St::Padding => "2px 6px",
                                    St::BorderRadius => "4px",
                                    St::FontWeight => "600",
                                },
                                "Required"
                            ],
                        ],
                        p![C!["welcome-modal__step-description"], 
                            "In Settings → Playback, select your audio interface and PCM device. This determines where your music will be played."],
                    ],
                ],
                div![
                    C!["welcome-modal__step"],
                    div![C!["welcome-modal__step-number"], "2"],
                    div![
                        C!["welcome-modal__step-content"],
                        div![
                            style! { St::Display => "flex", St::AlignItems => "center", St::Gap => "0.5rem" },
                            div![C!["welcome-modal__step-title"], "Music Library"],
                            span![
                                style! {
                                    St::BackgroundColor => "#3273dc",
                                    St::Color => "#fff",
                                    St::FontSize => "0.7rem",
                                    St::Padding => "2px 6px",
                                    St::BorderRadius => "4px",
                                    St::FontWeight => "600",
                                },
                                "Recommended"
                            ],
                        ],
                        p![C!["welcome-modal__step-description"], 
                            "In Settings → Music Library, add directories containing your music files. Optional if you only use radio streaming."],
                    ],
                ],
                div![
                    C!["welcome-modal__step"],
                    div![C!["welcome-modal__step-number"], "3"],
                    div![
                        C!["welcome-modal__step-content"],
                        div![C!["welcome-modal__step-title"], "Start Listening"],
                        p![C!["welcome-modal__step-description"], 
                            "Browse your library, add songs to queue, and enjoy! Use keyboard shortcuts (? for help, Space to play/pause)."],
                    ],
                ],
            ],
            // Actions
            div![
                C!["welcome-modal__actions"],
                a![
                    C!["welcome-modal__btn", "welcome-modal__btn--primary"],
                    attrs! { At::Href => "#/settings" },
                    "Go to Settings",
                    ev(Ev::Click, |_| Msg::DismissWelcomeModal)
                ],
                button![
                    C!["welcome-modal__btn", "welcome-modal__btn--secondary"],
                    "Dismiss",
                    ev(Ev::Click, |_| Msg::DismissWelcomeModal)
                ],
            ],
        ],
    ]
}

fn view_keyboard_shortcuts_help() -> Node<Msg> {
    let shortcuts = vec![
        ("?", "Show / Hide this help"),
        ("/", "Focus search field"),
        ("Space", "Play / Pause"),
        ("← / →", "Previous / Next track"),
        ("Shift+← / →", "Seek back / forward 10s"),
        ("↑ / ↓", "Volume up / down"),
        ("M", "Mute / Unmute"),
        ("L", "Like / Unlike track"),
        ("Y", "Toggle lyrics"),
        ("S", "Shuffle / Repeat mode"),
        ("Esc", "Close modal"),
    ];
    
    let nav_shortcuts = vec![
        ("1", "Now Playing"),
        ("2", "Queue"),
        ("3", "Library"),
        ("4", "Settings"),
        ("F", "Library Files"),
        ("A", "Library Artists"),
        ("P", "Library Playlists"),
        ("R", "Library Radio"),
        ("T", "Library Statistics"),
    ];
    
    div![
        C!["modal", "is-active"],
        div![
            C!["modal-background"],
            ev(Ev::Click, |_| Msg::ToggleKeyboardShortcutsHelp)
        ],
        div![
            C!["modal-content"],
            style! {
                St::BackgroundColor => "var(--ui-elements)",
                St::Padding => "2rem",
                St::BorderRadius => "12px",
                St::MaxWidth => "450px",
                St::BoxShadow => "0 20px 60px rgba(0, 0, 0, 0.5)",
            },
            // Header
            div![
                C!["keyboard-help__header"],
                style! { St::TextAlign => "center", St::MarginBottom => "1.5rem" },
                i![
                    C!["material-icons"],
                    style! { St::FontSize => "48px", St::Color => "var(--accent)", St::MarginBottom => "12px" },
                    "keyboard"
                ],
                h2![
                    style! { St::FontSize => "1.5rem", St::FontWeight => "700", St::Color => "var(--primary-text)" },
                    "Keyboard Shortcuts"
                ],
            ],
            // Shortcuts list
            div![
                style! {
                    St::Display => "flex",
                    St::FlexDirection => "column",
                    St::Gap => "12px",
                    St::MarginBottom => "1.5rem",
                },
                shortcuts.into_iter().map(|(key, description)| {
                    div![
                        style! {
                            St::Display => "flex",
                            St::AlignItems => "center",
                            St::JustifyContent => "space-between",
                            St::Padding => "10px 0",
                            St::BorderBottom => "1px solid var(--border-color)",
                        },
                        span![
                            style! { St::Color => "var(--secondary-text)", St::FontSize => "0.95rem" },
                            description
                        ],
                        kbd![
                            style! {
                                St::BackgroundColor => "var(--accent)",
                                St::Color => "var(--primary-text)",
                                St::Padding => "4px 12px",
                                St::BorderRadius => "4px",
                                St::FontFamily => "monospace",
                                St::FontSize => "0.85rem",
                                St::FontWeight => "600",
                                St::MinWidth => "40px",
                                St::TextAlign => "center",
                            },
                            key
                        ],
                    ]
                })
            ],
            // Navigation shortcuts section
            h3![
                style! { 
                    St::FontSize => "1.1rem", 
                    St::FontWeight => "600", 
                    St::Color => "var(--primary-text)",
                    St::MarginTop => "20px",
                    St::MarginBottom => "10px",
                    St::TextAlign => "center",
                },
                "Navigation"
            ],
            div![
                style! {
                    St::Display => "flex",
                    St::FlexDirection => "column",
                    St::Gap => "12px",
                    St::MarginBottom => "1.5rem",
                },
                nav_shortcuts.into_iter().map(|(key, description)| {
                    div![
                        style! {
                            St::Display => "flex",
                            St::AlignItems => "center",
                            St::JustifyContent => "space-between",
                            St::Padding => "10px 0",
                            St::BorderBottom => "1px solid var(--border-color)",
                        },
                        span![
                            style! { St::Color => "var(--secondary-text)", St::FontSize => "0.95rem" },
                            description
                        ],
                        kbd![
                            style! {
                                St::BackgroundColor => "var(--ui-elements)",
                                St::Color => "var(--primary-text)",
                                St::Border => "1px solid var(--border-color)",
                                St::Padding => "4px 12px",
                                St::BorderRadius => "4px",
                                St::FontFamily => "monospace",
                                St::FontSize => "0.85rem",
                                St::FontWeight => "600",
                                St::MinWidth => "40px",
                                St::TextAlign => "center",
                            },
                            key
                        ],
                    ]
                })
            ],
            // Close button
            div![
                style! { St::TextAlign => "center" },
                button![
                    C!["button", "is-primary"],
                    "Got it!",
                    ev(Ev::Click, |_| Msg::ToggleKeyboardShortcutsHelp)
                ],
            ],
        ],
        // Close on Escape
        ev(Ev::KeyDown, |event| {
            let keyboard_event: web_sys::KeyboardEvent = event.unchecked_into();
            if keyboard_event.key() == "Escape" {
                Msg::ToggleKeyboardShortcutsHelp
            } else {
                Msg::Ignore
            }
        }),
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
                            attrs! {"src" => get_background_image(player_model).unwrap_or("/no_album.svg".to_string())}
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
                        div![
                            C!["footer-volume"],
                            input![
                                C!["slider", "is-fullwidth"],
                                attrs! {"value"=> player_model.volume_state.current},
                                // attrs! {"step"=> player_model.volume_state.step},
                                attrs! {"max"=> player_model.volume_state.max},
                                attrs! {"min"=> player_model.volume_state.min},
                                attrs! {"type"=> "range"},
                                input_ev(Ev::Input, Msg::SetVolumeInput),
                                input_ev(Ev::Change, Msg::SetVolume),
                            ],
                        ],
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
        C!["main-content"],
        match page {
            Page::Home => page::home::view(base_url),
            Page::NotFound => page::not_found::view(),
            Page::Settings(model) => page::settings::view(model, &main_model.current_theme).map_msg(Msg::Settings),
            Page::Player => page::player::view(&main_model.player_model),
            Page::Queue(model) => page::queue::view(model).map_msg(Msg::Queue),
            Page::MusicLibraryStaticPlaylist(model) =>
                page::music_library_static_playlist::view(model).map_msg(Msg::MusicLibraryStaticPlaylist),
            Page::MusicLibraryFiles(model) => page::music_library_files::view(model).map_msg(Msg::MusicLibraryFiles),
            Page::MusicLibraryArtists(model) =>
                page::music_library_artists::view(model).map_msg(Msg::MusicLibraryArtists),
            Page::MusicLibraryRadio(model) => page::music_library_radio::view(model).map_msg(Msg::MusicLibraryRadio),
            Page::LibraryStats(model) => page::library_stats::view(model).map_msg(Msg::LibraryStats),
        }
    ]
}

fn view_navigation_tabs(page: &Page, library_nav_open: bool) -> Node<Msg> {
    let page_name: &str = page.into();
    let is_library = matches!(
        page,
        Page::MusicLibraryFiles(_)
            | Page::MusicLibraryStaticPlaylist(_)
            | Page::MusicLibraryArtists(_)
            | Page::MusicLibraryRadio(_)
            | Page::LibraryStats(_)
    );

    nav![
        C!["app-nav", "has-background-dark-transparent"],
        // Main tab row
        ul![
            C!["app-nav__items"],
            // Now Playing
            li![
                C!["app-nav__item", IF!(page_name == "Player" => "is-active")],
                a![
                    C!["app-nav__link"],
                    attrs!("title" => "Now Playing (1)"),
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "music_note"],
                    span![C!["app-nav__label"], "Now Playing"],
                    ev(Ev::Click, |_| Urls::player_abs().go_and_load()),
                ],
            ],
            // Queue
            li![
                C!["app-nav__item", IF!(page_name == "Queue" => "is-active")],
                a![
                    C!["app-nav__link"],
                    attrs!("title" => "Queue (2)"),
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "queue_music"],
                    span![C!["app-nav__label"], "Queue"],
                    ev(Ev::Click, |_| Urls::queue_abs().go_and_load()),
                ],
            ],
            // Library — clicking toggles the sub-nav dropdown
            li![
                C!["app-nav__item", IF!(is_library => "is-active")],
                a![
                    C!["app-nav__link"],
                    attrs!("title" => "Library (3)"),
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "library_music"],
                    span![C!["app-nav__label"], "Library"],
                    // Chevron rotates when open
                    i![
                        C!["material-icons", "app-nav__chevron", IF!(library_nav_open => "is-open")],
                        attrs!("aria-hidden" => "true"),
                        "expand_more"
                    ],
                    ev(Ev::Click, |_| Msg::ToggleLibraryNav),
                ],
            ],
            // Settings
            li![
                C!["app-nav__item", IF!(page_name == "Settings" => "is-active")],
                a![
                    C!["app-nav__link"],
                    attrs!("title" => "Settings (4)"),
                    i![C!["material-icons"], attrs!("aria-hidden" => "true"), "tune"],
                    span![C!["app-nav__label"], "Settings"],
                    ev(Ev::Click, |_| Urls::settings_abs().go_and_load()),
                ],
            ],
        ],
        // Library sub-nav — slides in below the main row when open
        IF!(library_nav_open =>
            div![
                C!["app-nav__subnav"],
                a![
                    C!["app-nav__sublink", IF!(page_name == "MusicLibraryStaticPlaylist" => "is-active")],
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_PL_STATIC), "title" => "Playlists (P)"},
                    i![C!["material-icons"], "playlist_play"],
                    span!["Playlists"],
                ],
                a![
                    C!["app-nav__sublink", IF!(page_name == "MusicLibraryFiles" => "is-active")],
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_FILES), "title" => "Files (F)"},
                    i![C!["material-icons"], "folder_open"],
                    span!["Files"],
                ],
                a![
                    C!["app-nav__sublink", IF!(page_name == "MusicLibraryArtists" => "is-active")],
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_ARTISTS), "title" => "Artists (A)"},
                    i![C!["material-icons"], "people"],
                    span!["Artists"],
                ],
                a![
                    C!["app-nav__sublink", IF!(page_name == "MusicLibraryRadio" => "is-active")],
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_RADIO), "title" => "Radio (R)"},
                    i![C!["material-icons"], "radio"],
                    span!["Radio"],
                ],
                a![
                    C!["app-nav__sublink", IF!(page_name == "LibraryStats" => "is-active")],
                    attrs! {At::Href => Urls::library_abs().add_hash_path_part(MUSIC_LIBRARY_STATS), "title" => "Statistics (T)"},
                    i![C!["material-icons"], "insert_chart"],
                    span!["Statistics"],
                ],
            ]
        ),
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
    pub fn attachQueueDragScroll();

    /// Advance to the next theme in the cycle, persist to localStorage, and
    /// return the new theme name.
    pub fn cycleTheme() -> String;

    /// Return the currently active theme name (reads localStorage / system pref).
    pub fn getTheme() -> String;

    /// Apply a named theme, persist to localStorage, return the applied name.
    pub fn applyTheme(theme: &str) -> String;

    /// Return JSON string of all theme names in order.
    pub fn getAllThemes() -> String;

    /// Return JSON string of full theme metadata map (label, bg, text, accent, ui).
    pub fn getAllThemeMeta() -> String;

    /// Update browser Media Session metadata (title, artist, album art).
    pub fn updateMediaSessionMetadata(title: &str, artist: &str, album: &str, artwork_url: &str);

    /// Register Media Session action handlers for media keys (play/pause/next/prev/seek).
    pub fn setupMediaSessionHandlers();
    
    /// Check if this is the first time the user visits the app.
    pub fn isFirstVisit() -> bool;
    
    /// Mark that user has visited the app (dismissed welcome modal).
    pub fn markVisited();
    
    /// Navigate to a hash URL safely (defers with setTimeout to avoid borrow issues).
    pub fn navigateToHash(hash: &str);
    
    /// Focus on the search input field if present. Returns true if focused.
    pub fn focusSearchInput() -> bool;
    
    /// Focus on the audio interface select dropdown in settings. Returns true if focused.
    pub fn focusAudioInterfaceSelect() -> bool;

    /// Register or unregister the browser beforeunload warning for unsaved changes.
    pub fn setBeforeUnloadWarning(has_changes: bool);

    /// Persist a flag so the Playback settings section stays open after a forced reload.
    pub fn setPlaybackSectionOpen(value: bool);

    /// Read and clear the one-shot flag for keeping the Playback section open.
    pub fn getAndClearPlaybackSectionOpen() -> bool;
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
    if let Some(album) = track.album {
        if let Some(artist) = track.artist {
            let ai = get_album_image_from_lastfm_api(album, artist).await;
            return ai.map_or_else(|| Msg::Ignore, Msg::AlbumImageUpdated);
        }
    }
    Msg::Ignore
}

#[allow(clippy::future_not_send)]
async fn fetch_lyrics_task(song: Song) -> Msg {
    let artist = song.artist.as_deref().unwrap_or_default();
    let title = song.title.as_deref().unwrap_or_default();
    let album = song.album.as_deref().unwrap_or_default();
    let duration = song.time.map(|d| d.as_secs()).unwrap_or(0);

    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
        js_sys::encode_uri_component(artist),
        js_sys::encode_uri_component(title),
        js_sys::encode_uri_component(album),
        duration
    );

    let res = Request::get(&url).send().await;
    match res {
        Ok(response) => {
            if response.status() == 200 {
                let lyrics = response.json::<lyrics::LrcLibResponse>().await.ok();
                Msg::LyricsFetched(lyrics)
            } else {
                Msg::LyricsFetched(None)
            }
        }
        Err(_) => Msg::LyricsFetched(None),
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

const VISUALIZER_STORAGE_KEY: &str = "rsplayer_visualizer";

fn load_visualizer_type() -> vumeter::VisualizerType {
    (|| {
        let storage = web_sys::window()?.local_storage().ok()??;
        let value = storage.get_item(VISUALIZER_STORAGE_KEY).ok()??;
        vumeter::VisualizerType::from_str(&value)
    })()
    .unwrap_or(vumeter::VisualizerType::Lissajous)
}

fn save_visualizer_type(vt: vumeter::VisualizerType) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(VISUALIZER_STORAGE_KEY, vt.as_str());
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

fn view_local_browser_playback(model: &Model) -> Node<Msg> {
    if !model.local_browser_playback {
        return empty!();
    }
    
    let is_playing = model.player_model.player_state == PlayerState::PLAYING;
    let file = model.player_model.current_song.as_ref().map(|s| s.file.clone());

    if let Some(file_path) = file {
        let src = if file_path.starts_with("http://") || file_path.starts_with("https://") {
            file_path
        } else {
            let encoded_path = js_sys::encode_uri_component(&file_path);
            // encodeURIComponent encodes `/` as `%2F`, which we don't want for paths
            let encoded_path = String::from(encoded_path).replace("%2F", "/");
            format!("/music/{}", encoded_path)
        };
        
        div![
            C!["local-playback-container"],
            style! { St::Display => "none" },
            seed::custom![
                Tag::from("audio"),
                attrs! {
                    At::Id => "local-audio-player",
                    At::Src => src,
                    At::AutoPlay => is_playing.as_at_value(),
                    At::Controls => true.as_at_value(),
                },
                ev(Ev::from("timeupdate"), |event: web_sys::Event| {
                    let audio: web_sys::HtmlAudioElement = event.target().unwrap().unchecked_into();
                    let current = audio.current_time();
                    let duration = audio.duration();
                    if duration.is_nan() || duration == 0.0 {
                        Msg::Ignore
                    } else {
                        Msg::BrowserAudioTimeUpdate(current, duration)
                    }
                }),
                ev(Ev::from("ended"), |_| Msg::BrowserAudioEnded),
                ev(Ev::from("pause"), |_| Msg::BrowserAudioPaused),
                ev(Ev::from("play"), |_| Msg::BrowserAudioPlaying),
            ]
        ]
    } else {
        empty!()
    }
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
