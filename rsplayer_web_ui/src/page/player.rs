use api_models::{
    common::{MetadataCommand, PlaybackMode, PlayerCommand, SystemCommand, UserCommand, Volume},
    player::Song,
    state::{PlayerInfo, PlayerState, SongProgress},
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use web_sys::WebSocket;

use crate::{
    hooks::ws_send,
    lyrics::{self, LrcLibResponse, LyricLine},
    navigate, send_system_cmd,
    state::AppState,
    vumeter::{VUMeter, VisualizerType},
    CurrentPath, UiState,
};

// ─── Last.fm album art types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LastFmAlbumInfo {
    album: LastFmAlbum,
}

#[derive(Debug, Deserialize)]
struct LastFmAlbum {
    image: Vec<LastFmImage>,
}

#[derive(Debug, Deserialize)]
struct LastFmImage {
    size: String,
    #[serde(rename = "#text")]
    text: String,
}

// ─── Album art helper ────────────────────────────────────────────────────────

/// Returns a local artwork URL if the song has an embedded image, else None.
pub fn local_album_image(song: &Song) -> Option<String> {
    if let Some(image_id) = song.image_id.as_ref() {
        return Some(format!("/artwork/{}", image_id));
    }
    song.image_url.clone()
}

pub async fn fetch_album_cover(song: &Song) -> Option<String> {
    if let Some(image_id) = &song.image_id {
        return Some(format!("/artwork/{}", image_id));
    }
    let album = song.album.as_deref()?;
    let artist = song.artist.as_deref()?;
    let protocol = web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .unwrap_or_else(|| "http:".to_string());
    let url = format!(
        "{protocol}//ws.audioscrobbler.com/2.0/?api_key=3b3df6c5dd3ad07222adc8dd3ccd8cdc&format=json&method=album.getinfo&album={}&artist={}",
        js_sys::encode_uri_component(album),
        js_sys::encode_uri_component(artist),
    );
    let resp = Request::get(&url).send().await.ok()?;
    let info: LastFmAlbumInfo = resp.json().await.ok()?;
    info.album
        .image
        .into_iter()
        .find(|i| i.size == "mega" && !i.text.is_empty())
        .map(|i| i.text)
}

async fn fetch_lyrics(song: &Song) -> Option<LrcLibResponse> {
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
    let resp = Request::get(&url).send().await.ok()?;
    if resp.status() == 200 {
        resp.json().await.ok()
    } else {
        None
    }
}

// ─── Player Page ─────────────────────────────────────────────────────────────

#[component]
pub fn PlayerPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let current_song = state.current_song;
    let player_info = state.player_info;
    let progress = state.progress;
    let volume = state.volume;
    let player_state = state.player_state;
    let playback_mode = state.playback_mode;
    let vu_meter_enabled = state.vu_meter_enabled;

    let mut ui = use_context::<UiState>();
    let mut lyrics_data = use_signal(|| None::<Vec<LyricLine>>);
    let mut plain_lyrics = use_signal(|| None::<String>);
    let mut lyrics_loading = use_signal(|| false);

    // Reset lyrics when song changes
    use_effect(move || {
        let _ = current_song.read(); // subscribe
        lyrics_data.set(None);
        plain_lyrics.set(None);
    });

    // Fetch lyrics when lyrics panel opens
    use_effect(move || {
        if (ui.lyrics_open)() && lyrics_data.peek().is_none() && plain_lyrics.peek().is_none() {
            if let Some(song) = current_song.peek().clone() {
                *lyrics_loading.write() = true;
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(resp) = fetch_lyrics(&song).await {
                        if let Some(synced) = resp.synced_lyrics {
                            lyrics_data.set(Some(lyrics::parse_lrc(&synced)));
                        }
                        if let Some(plain) = resp.plain_lyrics {
                            plain_lyrics.set(Some(plain));
                        }
                    }
                    *lyrics_loading.write() = false;
                });
            }
        }
    });

    rsx! {
        div {
            class: "player-page",
            // VU meter canvas layer
            if *vu_meter_enabled.read() && *state.visualizer_type.read() != VisualizerType::None {
                VUMeterCanvas {}
            }
            // Content
            div { class: "player-page__content",
                TrackInfo {
                    song: current_song.read().clone(),
                    player_info: player_info.read().clone(),
                }
                Controls {
                    ws,
                    player_state: player_state.read().clone(),
                    playback_mode: *playback_mode.read(),
                    progress: progress.read().clone(),
                    volume: *volume.read(),
                    current_song: current_song.read().clone(),
                    vu_meter_enabled: *vu_meter_enabled.read(),
                    visualizer_type: state.visualizer_type,
                    on_lyrics: move |_| {
                        let open = *ui.lyrics_open.peek();
                        ui.lyrics_open.set(!open);
                    },
                }
            }
            if (ui.lyrics_open)() {
                LyricsModal {
                    on_close: move |_| ui.lyrics_open.set(false),
                    lyrics_data: lyrics_data.read().clone(),
                    plain_lyrics: plain_lyrics.read().clone(),
                    loading: *lyrics_loading.read(),
                    progress: progress.read().clone(),
                }
            }
        }
    }
}

// ─── VU Meter Canvas ─────────────────────────────────────────────────────────

#[component]
fn VUMeterCanvas() -> Element {
    let state = use_context::<AppState>();
    let vu_left = state.vu_left;
    let vu_right = state.vu_right;
    let visualizer_type = state.visualizer_type;

    let mut meter: Signal<Option<VUMeter>> = use_signal(|| None);

    use_effect(move || {
        let vt = *visualizer_type.read();
        if vt != VisualizerType::None {
            if let Some(m) = VUMeter::with_type("vumeter", vt) {
                meter.set(Some(m));
            }
        } else {
            meter.set(None);
        }
    });

    use_effect(move || {
        let l = *vu_left.read();
        let r = *vu_right.read();
        if let Some(ref mut m) = *meter.write() {
            m.update(l, r);
        }
    });

    rsx! {
        div {
            class: "player-page__vumeter",
            canvas {
                id: "vumeter",
            }
        }
    }
}

fn load_visualizer_type() -> VisualizerType {
    (|| {
        let storage = web_sys::window()?.local_storage().ok()??;
        let value = storage.get_item("rsplayer_visualizer").ok()??;
        VisualizerType::from_str(&value)
    })()
    .unwrap_or(VisualizerType::Lissajous)
}

fn save_visualizer_type(vt: VisualizerType) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("rsplayer_visualizer", vt.as_str());
        }
    }
}

// ─── Track Info ──────────────────────────────────────────────────────────────

#[component]
fn TrackInfo(song: Option<Song>, player_info: Option<PlayerInfo>) -> Element {
    let CurrentPath(path) = use_context::<CurrentPath>();
    match song {
        None => rsx! {
            div { class: "track-info text-center py-8",
                div { class: "skeleton-player",
                    div { class: "skeleton skeleton-player-image" }
                    div { class: "skeleton skeleton-player-title" }
                    div { class: "skeleton skeleton-player-artist" }
                }
                p { class: "text-base-content/30 text-sm mt-5",
                    "Ready to play — add songs from your library"
                }
            }
        },
        Some(ps) => {
            let title = ps.title.as_deref().unwrap_or("Unknown Track");
            let title_class = match title.len() {
                0..=19 => "text-4xl",
                20..=31 => "text-3xl",
                _ => "text-2xl",
            };
            let codec_info = player_info.as_ref().map_or_else(
                || "No file playing".to_string(),
                |pi| {
                    format!(
                        "{} - {} / {} Hz",
                        pi.codec.as_deref().unwrap_or(""),
                        pi.audio_format_bit.unwrap_or(0),
                        pi.audio_format_rate.unwrap_or(0),
                    )
                },
            );
            let loudness =
                player_info
                    .as_ref()
                    .and_then(|pi| match (pi.track_loudness_lufs, pi.normalization_gain_db) {
                        (Some(l), Some(g)) => Some(format!(
                            "{:.1} LUFS  →  {:+.1} dB  →  {:.1} LUFS",
                            l as f64 / 100.0,
                            g as f64 / 100.0,
                            (l + g) as f64 / 100.0
                        )),
                        (Some(l), None) => Some(format!("{:.1} LUFS", l as f64 / 100.0)),
                        (None, Some(g)) => Some(format!("{:+.1} dB (file tag)", g as f64 / 100.0)),
                        _ => None,
                    });
            let artist = ps.artist.clone();
            let album = ps.album.clone();
            rsx! {
                div { class: "track-info text-center",
                    h1 { class: "font-bold text-base-content {title_class} mb-1", "{title}" }
                    if let Some(ref artist) = artist {
                        a {
                            class: "text-xl text-base-content/80 hover:underline block cursor-pointer",
                            href: "/library/artists?search={artist}",
                            onclick: {
                                let artist = artist.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    navigate(path, &format!("/library/artists?search={artist}"));
                                }
                            },
                            "{artist}"
                        }
                    }
                    if let Some(ref album) = album {
                        a {
                            class: "text-base text-base-content/60 hover:underline block cursor-pointer",
                            href: "/library/files?search={album}",
                            onclick: {
                                let album = album.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    navigate(path, &format!("/library/files?search={album}"));
                                }
                            },
                            "{album}"
                        }
                    }
                    if let Some(genre) = &ps.genre {
                        p { class: "text-sm text-base-content/50", "{genre}" }
                    }
                    if let Some(date) = &ps.date {
                        p { class: "text-sm text-base-content/50", "{date}" }
                    }
                    p { class: "text-sm text-base-content/50 mt-1", "{codec_info}" }
                    if let Some(l) = loudness {
                        p { class: "text-xs text-base-content/40", "{l}" }
                    }
                }
            }
        }
    }
}

// ─── Controls ────────────────────────────────────────────────────────────────

#[component]
fn Controls(
    ws: Signal<Option<WebSocket>>,
    player_state: PlayerState,
    playback_mode: PlaybackMode,
    progress: SongProgress,
    volume: Volume,
    current_song: Option<Song>,
    on_lyrics: EventHandler,
    vu_meter_enabled: bool,
    visualizer_type: Signal<VisualizerType>,
) -> Element {
    let playing = player_state == PlayerState::PLAYING;
    let (shuffle_icon, shuffle_title) = match playback_mode {
        PlaybackMode::Sequential => ("format_list_numbered", "Sequential"),
        PlaybackMode::Random => ("shuffle", "Random"),
        PlaybackMode::LoopSingle => ("repeat_one", "Loop Single"),
        PlaybackMode::LoopQueue => ("repeat", "Loop Queue"),
    };
    let liked = current_song
        .as_ref()
        .and_then(|s| s.statistics.as_ref())
        .map_or(false, |st| st.liked_count > 0);
    let song_id = current_song.as_ref().map(|s| s.file.clone());
    let is_muted = volume.current == 0;

    rsx! {
        div { class: "player-controls",
            // ── Main controls row: prev / play-pause / next ───────────────────────────────────────
            div { class: "player-controls__main-row",
                button {
                    class: "btn btn-ghost btn-md",
                    title: "Previous",
                    onclick: { let ws = ws; move |_| ws_send(&ws, &UserCommand::Player(PlayerCommand::Prev)) },
                    i { class: "material-icons text-xl", "skip_previous" }
                }
                button {
                    class: "btn btn-primary btn-circle btn-lg",
                    title: if playing { "Pause" } else { "Play" },
                    onclick: {
                        let ws = ws;
                        move |_| ws_send(&ws, &UserCommand::Player(if playing { PlayerCommand::Pause } else { PlayerCommand::Play }))
                    },
                    i { class: "material-icons text-xl", if playing { "pause" } else { "play_arrow" } }
                }
                button {
                    class: "btn btn-ghost btn-md",
                    title: "Next",
                    onclick: { let ws = ws; move |_| ws_send(&ws, &UserCommand::Player(PlayerCommand::Next)) },
                    i { class: "material-icons text-xl", "skip_next" }
                }
            }
            // ── Secondary buttons row ───────────────────────────────────────
            div { class: "player-controls__secondary-row",
                if vu_meter_enabled {
                    button {
                        class: "btn btn-ghost btn-sm",
                        title: "Toggle visualizer (V)",
                        onclick: {
                            let mut vt = visualizer_type;
                            move |_| {
                                let current = *vt.read();
                                let next = current.cycle();
                                vt.set(next);
                                save_visualizer_type(next);
                            }
                        },
                        i { class: "material-icons", "equalizer" }
                    }
                }
                button {
                    class: "btn btn-ghost btn-sm",
                    title: "{shuffle_title}",
                    onclick: { let ws = ws; move |_| ws_send(&ws, &UserCommand::Player(PlayerCommand::CyclePlaybackMode)) },
                    i { class: "material-icons", "{shuffle_icon}" }
                }
                if let Some(ref id) = song_id {
                    button {
                        class: "btn btn-ghost btn-sm",
                        title: "Like / Unlike",
                        onclick: {
                            let ws = ws; let id = id.clone();
                            move |_| {
                                let cmd = if liked { MetadataCommand::DislikeMediaItem(id.clone()) }
                                          else     { MetadataCommand::LikeMediaItem(id.clone()) };
                                ws_send(&ws, &UserCommand::Metadata(cmd));
                            }
                        },
                        i { class: if liked { "material-icons text-error" } else { "material-icons" }, if liked { "favorite" } else { "favorite_border" } }
                    }
                }
                button {
                    class: "btn btn-ghost btn-sm",
                    title: "Lyrics",
                    onclick: move |_| on_lyrics.call(()),
                    i { class: "material-icons", "lyrics" }
                }
                button {
                    class: "btn btn-ghost btn-sm",
                    title: if is_muted { "Unmute" } else { "Mute" },
                    onclick: { let ws = ws; move |_| send_system_cmd(&ws, SystemCommand::ToggleMute) },
                    i { class: "material-icons", if is_muted { "volume_off" } else { "volume_up" } }
                }
            }
            SeekBar { ws, progress }
            VolumeControl { ws, volume }
        }
    }
}

// ─── Seek Bar ────────────────────────────────────────────────────────────────

#[component]
fn SeekBar(ws: Signal<Option<WebSocket>>, progress: SongProgress) -> Element {
    let cur = progress.current_time.as_secs();
    let tot = progress.total_time.as_secs();
    rsx! {
        div { class: "player-controls__seek",
            div { class: "flex justify-between text-xs text-base-content/60 mb-1",
                span { "{format_time(cur)}" }
                span { "{format_time(tot)}" }
            }
            input {
                r#type: "range",
                class: "range range-primary range-xs w-full",
                min: 0, max: tot as i64, value: cur as i64,
                aria_label: "Track progress",
                onchange: { let ws = ws; move |e: Event<FormData>| {
                    if let Ok(v) = e.value().parse::<u16>() {
                        ws_send(&ws, &UserCommand::Player(PlayerCommand::Seek(v)));
                    }
                }},
            }
        }
    }
}

// ─── Volume Control ──────────────────────────────────────────────────────────

#[component]
fn VolumeControl(ws: Signal<Option<WebSocket>>, volume: Volume) -> Element {
    let pct = if volume.max > 0 {
        (volume.current as f32 / volume.max as f32 * 100.0) as u8
    } else {
        0
    };
    rsx! {
        div { class: "player-controls__volume",
            div { class: "player-controls__volume-row",
                button {
                    class: "btn btn-ghost btn-sm",
                    title: "Volume down",
                    onclick: { let ws = ws; move |_| send_system_cmd(&ws, SystemCommand::VolDown) },
                    i { class: "material-icons", "remove_circle" }
                }
                input {
                    r#type: "range",
                    class: "range range-sm flex-1",
                    min: volume.min as i64, max: volume.max as i64, value: volume.current as i64,
                    aria_label: "Volume",
                    onchange: { let ws = ws; move |e: Event<FormData>| {
                        if let Ok(v) = e.value().parse::<u8>() {
                            send_system_cmd(&ws, SystemCommand::SetVol(v));
                        }
                    }},
                }
                button {
                    class: "btn btn-ghost btn-sm",
                    title: "Volume up",
                    onclick: { let ws = ws; move |_| send_system_cmd(&ws, SystemCommand::VolUp) },
                    i { class: "material-icons", "add_circle" }
                }
            }
            span { class: "text-sm text-base-content/60", "Volume: {pct}%" }
        }
    }
}

// ─── Lyrics Modal ────────────────────────────────────────────────────────────

#[component]
fn LyricsModal(
    on_close: EventHandler,
    lyrics_data: Option<Vec<LyricLine>>,
    plain_lyrics: Option<String>,
    loading: bool,
    progress: SongProgress,
) -> Element {
    let current_time = progress.current_time.as_secs_f64();
    let active_index = lyrics_data
        .as_ref()
        .and_then(|lines| lines.iter().rposition(|line| line.time_secs <= current_time));

    rsx! {
        div { class: "modal modal-open",
            div { class: "modal-backdrop", onclick: move |_| on_close.call(()) }
            div { class: "modal-box max-w-lg max-h-[80vh] overflow-y-auto bg-base-300",
                button {
                    class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
                if loading {
                    div { class: "text-center py-8 text-base-content/50", "Loading lyrics..." }
                } else if let Some(ref lines) = lyrics_data {
                    div { class: "lyrics-list py-4",
                        {lines.iter().enumerate().map(|(idx, line)| {
                            let is_active = Some(idx) == active_index;
                            rsx! {
                                div {
                                    key: "lyric-{idx}",
                                    id: if is_active { "lyric-active" } else { "" },
                                    class: if is_active { "lyric-line is-active" } else { "lyric-line" },
                                    "{line.text}"
                                }
                            }
                        })}
                    }
                } else if let Some(ref plain) = plain_lyrics {
                    div {
                        class: "py-4 text-center text-lg",
                        style: "white-space: pre-wrap;",
                        "{plain}"
                    }
                } else {
                    div { class: "text-center py-8 text-base-content/50", "Lyrics not found." }
                }
            }
        }
    }
}

pub fn format_time(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
