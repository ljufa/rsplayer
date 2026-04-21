use api_models::{
    common::{MetadataCommand, StorageCommand, SystemCommand, UserCommand, VolumeCrtlType},
    settings::{
        DspFilter, DspSettings, FilterConfig, NetworkMountConfig, NetworkMountType, NormalizationSource, Settings,
    },
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use web_sys::WebSocket;

use crate::dsp::get_dsp_presets;
use crate::{hooks::ws_send, send_system_cmd, state::AppState};

const API_SETTINGS_PATH: &str = "/api/settings";

// ─── Local state helpers ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ConfirmAction {
    FullRescan,
    RestartPlayer,
    RestartSystem,
    ShutdownSystem,
    RemoveMusicDirectory(usize),
    RemoveNetworkMount(String),
    ClearDspFilters,
}

#[derive(Debug, Clone, PartialEq, EnumIter)]
enum DspFilterType {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
    LowPassFO,
    HighPassFO,
    LowShelfFO,
    HighShelfFO,
    Gain,
}

impl DspFilterType {
    fn label(&self) -> &'static str {
        match self {
            DspFilterType::Peaking => "Peaking",
            DspFilterType::LowShelf => "LowShelf",
            DspFilterType::HighShelf => "HighShelf",
            DspFilterType::LowPass => "LowPass",
            DspFilterType::HighPass => "HighPass",
            DspFilterType::BandPass => "BandPass",
            DspFilterType::Notch => "Notch",
            DspFilterType::AllPass => "AllPass",
            DspFilterType::LowPassFO => "LowPassFO",
            DspFilterType::HighPassFO => "HighPassFO",
            DspFilterType::LowShelfFO => "LowShelfFO",
            DspFilterType::HighShelfFO => "HighShelfFO",
            DspFilterType::Gain => "Gain",
        }
    }
    fn default_filter(&self) -> DspFilter {
        match self {
            DspFilterType::Peaking => DspFilter::Peaking {
                freq: 1000.0,
                gain: 0.0,
                q: 0.707,
            },
            DspFilterType::LowShelf => DspFilter::LowShelf {
                freq: 80.0,
                gain: 0.0,
                q: Some(0.707),
                slope: None,
            },
            DspFilterType::HighShelf => DspFilter::HighShelf {
                freq: 12000.0,
                gain: 0.0,
                q: Some(0.707),
                slope: None,
            },
            DspFilterType::LowPass => DspFilter::LowPass {
                freq: 20000.0,
                q: 0.707,
            },
            DspFilterType::HighPass => DspFilter::HighPass { freq: 20.0, q: 0.707 },
            DspFilterType::BandPass => DspFilter::BandPass { freq: 1000.0, q: 0.707 },
            DspFilterType::Notch => DspFilter::Notch { freq: 1000.0, q: 0.707 },
            DspFilterType::AllPass => DspFilter::AllPass { freq: 1000.0, q: 0.707 },
            DspFilterType::LowPassFO => DspFilter::LowPassFO { freq: 20000.0 },
            DspFilterType::HighPassFO => DspFilter::HighPassFO { freq: 20.0 },
            DspFilterType::LowShelfFO => DspFilter::LowShelfFO { freq: 80.0, gain: 0.0 },
            DspFilterType::HighShelfFO => DspFilter::HighShelfFO {
                freq: 12000.0,
                gain: 0.0,
            },
            DspFilterType::Gain => DspFilter::Gain { gain: 0.0 },
        }
    }
}

fn filter_type_of(f: &DspFilter) -> DspFilterType {
    match f {
        DspFilter::Peaking { .. } => DspFilterType::Peaking,
        DspFilter::LowShelf { .. } => DspFilterType::LowShelf,
        DspFilter::HighShelf { .. } => DspFilterType::HighShelf,
        DspFilter::LowPass { .. } => DspFilterType::LowPass,
        DspFilter::HighPass { .. } => DspFilterType::HighPass,
        DspFilter::BandPass { .. } => DspFilterType::BandPass,
        DspFilter::Notch { .. } => DspFilterType::Notch,
        DspFilter::AllPass { .. } => DspFilterType::AllPass,
        DspFilter::LowPassFO { .. } => DspFilterType::LowPassFO,
        DspFilter::HighPassFO { .. } => DspFilterType::HighPassFO,
        DspFilter::LowShelfFO { .. } => DspFilterType::LowShelfFO,
        DspFilter::HighShelfFO { .. } => DspFilterType::HighShelfFO,
        DspFilter::Gain { .. } | DspFilter::LinkwitzTransform { .. } => DspFilterType::Gain,
    }
}

// ─── Page ─────────────────────────────────────────────────────────────────────

#[component]
pub fn SettingsPage() -> Element {
    let state = use_context::<AppState>();
    let ws = use_context::<Signal<Option<WebSocket>>>();

    let mut settings: Signal<Settings> = use_signal(Settings::default);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut confirm: Signal<Option<ConfirmAction>> = use_signal(|| None);
    let mut dsp_dirty = use_signal(|| false);
    let mut pending_restart = use_signal(|| false);

    // Fetch settings on mount
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = Request::get(API_SETTINGS_PATH).send().await {
                if let Ok(s) = resp.json::<Settings>().await {
                    *settings.write() = s;
                }
            }
            *loading.write() = false;
            // Query mount/dir status
            ws_send(&ws, &UserCommand::Storage(StorageCommand::QueryMountStatus));
            ws_send(&ws, &UserCommand::Storage(StorageCommand::QueryMusicDirStatus));
        });
    });

    let mut auto_save = move || {
        *saving.write() = true;
        let s = settings.read().clone();
        spawn(async move {
            let _ = Request::post(API_SETTINGS_PATH)
                .json(&s)
                .expect("serialize settings")
                .send()
                .await;
            *saving.write() = false;
        });
    };

    let mut auto_save_restart = move || {
        *pending_restart.write() = true;
        auto_save();
    };

    let mut do_confirm = move || {
        let action = confirm.read().clone();
        match action {
            Some(ConfirmAction::FullRescan) => {
                let s = settings.read().clone();
                spawn(async move {
                    let _ = Request::post(API_SETTINGS_PATH).json(&s).expect("s").send().await;
                });
                ws_send(
                    &ws,
                    &UserCommand::Metadata(MetadataCommand::RescanMetadata(String::new(), true)),
                );
            }
            Some(ConfirmAction::RestartPlayer) => {
                *pending_restart.write() = false;
                send_system_cmd(&ws, SystemCommand::RestartRSPlayer);
            }
            Some(ConfirmAction::RestartSystem) => {
                send_system_cmd(&ws, SystemCommand::RestartSystem);
            }
            Some(ConfirmAction::ShutdownSystem) => {
                send_system_cmd(&ws, SystemCommand::PowerOff);
            }
            Some(ConfirmAction::RemoveMusicDirectory(idx)) => {
                let mut s = settings.write();
                s.metadata_settings.music_directories.remove(idx);
                drop(s);
                auto_save();
            }
            Some(ConfirmAction::RemoveNetworkMount(ref name)) => {
                let mount_point = settings
                    .read()
                    .network_storage_settings
                    .mounts
                    .iter()
                    .find(|m| &m.name == name)
                    .map(|m| {
                        m.mount_point
                            .clone()
                            .unwrap_or_else(|| format!("/mnt/rsplayer/{}", m.name))
                    })
                    .unwrap_or_default();
                {
                    let mut s = settings.write();
                    s.network_storage_settings.mounts.retain(|m| &m.name != name);
                    s.metadata_settings.music_directories.retain(|d| d != &mount_point);
                }
                ws_send(&ws, &UserCommand::Storage(StorageCommand::Remove(name.clone())));
                auto_save();
            }
            Some(ConfirmAction::ClearDspFilters) => {
                settings.write().rs_player_settings.dsp_settings.filters.clear();
                *dsp_dirty.write() = true;
            }
            None => {}
        }
        *confirm.write() = None;
    };

    if loading() {
        return rsx! {
            div { class: "flex items-center justify-center py-20",
                span { class: "loading loading-spinner loading-lg" }
            }
        };
    }

    rsx! {
        div { class: "max-w-2xl mx-auto px-3 py-4 pb-20 space-y-3",

            // ── Header ──────────────────────────────────────────────────────
            div { class: "flex items-center justify-between",
                // h1 { class: "text-xl font-bold", "Settings" }
                if saving() {
                    span { class: "loading loading-spinner loading-sm text-primary" }
                }
            }

            // ── Appearance section ───────────────────────────────────────────
            SettingsSection {
                title: "Appearance",
                icon: "palette",
                content: rsx! {
                    AppearanceSection {}
                },
            }

            // ── Playback section ─────────────────────────────────────────────
            SettingsSection {
                title: "Playback",
                icon: "speaker",
                content: rsx! {
                    // Audio card selection
                    div { class: "form-control mb-3",
                        label { class: "label",
                            span { class: "label-text font-medium", "Audio interface" }
                        }
                        select {
                            class: "select select-bordered select-sm w-full",
                            id: "audio-interface-select",
                            onchange: {
                                move |e: Event<FormData>| {
                                    let val = e.value();
                                    if val == "browser" {
                                        settings.write().local_browser_playback = true;
                                    } else {
                                        {
                                            let mut s = settings.write();
                                            s.local_browser_playback = false;
                                            s.alsa_settings.output_device.card_id = val.clone();
                                            if val == "pipewire" {
                                                s.volume_ctrl_settings.ctrl_device = VolumeCrtlType::Pipewire;
                                            } else {
                                                s.volume_ctrl_settings.ctrl_device = VolumeCrtlType::Alsa;
                                            }
                                        }
                                    }
                                    auto_save_restart();
                                }
                            },
                            option { value: "--", "-- Select audio card --" }
                            option {
                                value: "browser",
                                selected: settings.read().local_browser_playback,
                                "Browser (local playback)"
                            }
                            {
                                let cards = settings.read().alsa_settings.available_audio_cards.clone();
                                cards
                                    .into_iter()
                                    .map(|card| {
                                        let selected = settings.read().alsa_settings.output_device.card_id
                                            == card.id;
                                        rsx! {
                                            option { value: "{card.id}", selected, "{card.name} - {card.description}" }
                                        }
                                    })
                            }
                        }
                    }

                    // PCM device (only shown when a card is selected and not browser)
                    {
                        let card_id = settings.read().alsa_settings.output_device.card_id.clone();
                        let pcms = settings.read().alsa_settings.find_pcms_by_card_id(&card_id);
                        let show = !card_id.is_empty() && !card_id.starts_with("--")
                            && !settings.read().local_browser_playback;
                        if show && !pcms.is_empty() {
                            rsx! {
                                div { class: "form-control mb-3",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "PCM output device" }
                                    }
                                    select {
                                        class: "select select-bordered select-sm w-full",
                                        onchange: move |e: Event<FormData>| {
                                            let pcm_name = e.value();
                                            let cid = settings.read().alsa_settings.output_device.card_id.clone();
                                            settings.write().alsa_settings.set_output_device(&cid, &pcm_name);
                                            auto_save_restart();
                                        },
                                        {
                                            let current_pcm = settings.read().alsa_settings.output_device.name.clone();
                                            pcms.into_iter()
                                                .map(move |pcm| {
                                                    let selected = pcm.name == current_pcm;
                                                    rsx! {
                                                        option { value: "{pcm.name}", selected, "{pcm.description}" }
                                                    }
                                                })
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }

                    // Auto-resume
                    ToggleRow {
                        label: "Auto-resume playback on startup",
                        checked: settings.read().auto_resume_playback,
                        onchange: move |_| {
                            let v = !settings.read().auto_resume_playback;
                            settings.write().auto_resume_playback = v;
                            auto_save();
                        },
                    }

                    // RSPlayer advanced
                    div { class: "divider text-xs text-base-content/40 my-2", "RSPlayer Engine" }
                    NumberInput {
                        label: "Input buffer (MB)",
                        value: settings.read().rs_player_settings.input_stream_buffer_size_mb.to_string(),
                        min: "1",
                        max: "200",
                        onchange: move |v: String| {
                            if let Ok(n) = v.parse::<usize>() {
                                settings.write().rs_player_settings.input_stream_buffer_size_mb = n;
                            }
                        },
                    }
                    NumberInput {
                        label: "Ring buffer (ms)",
                        value: settings.read().rs_player_settings.ring_buffer_size_ms.to_string(),
                        min: "100",
                        max: "10000",
                        onchange: move |v: String| {
                            if let Ok(n) = v.parse::<usize>() {
                                settings.write().rs_player_settings.ring_buffer_size_ms = n;
                            }
                        },
                    }
                    NumberInput {
                        label: "Thread priority (1-99)",
                        value: settings.read().rs_player_settings.player_threads_priority.to_string(),
                        min: "1",
                        max: "99",
                        onchange: move |v: String| {
                            if let Ok(n) = v.parse::<u8>() {
                                settings.write().rs_player_settings.player_threads_priority = n;
                            }
                        },
                    }
                    NumberInput {
                        label: "ALSA buffer size (frames, 0=default)",
                        value: settings.read().rs_player_settings.alsa_buffer_size.unwrap_or(0).to_string(),
                        min: "0",
                        max: "100000",
                        onchange: move |v: String| {
                            let n = v.parse::<u32>().unwrap_or(0);
                            settings.write().rs_player_settings.alsa_buffer_size = if n == 0 {
                                None
                            } else {
                                Some(n)
                            };
                        },
                    }
                    div { class: "form-control mb-2",
                        label { class: "label py-0.5",
                            span { class: "label-text text-sm", "Fixed output sample rate" }
                        }
                        select {
                            class: "select select-sm select-bordered w-full",
                            onchange: move |e: Event<FormData>| {
                                let n = e.value().parse::<u32>().unwrap_or(0);
                                settings.write().rs_player_settings.fixed_output_sample_rate = if n == 0 {
                                    None
                                } else {
                                    Some(n)
                                };
                                auto_save_restart();
                            },
                            {
                                let current = settings
                                    .read()
                                    .rs_player_settings
                                    .fixed_output_sample_rate
                                    .unwrap_or(0);
                                [
                                    0u32, 44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000, 705600,
                                    768000,
                                ]
                                    .iter()
                                    .map(move |&v| {
                                        rsx! {
                                            option { value: "{v}", selected: current == v,
                                                if v == 0 {
                                                    "Off"
                                                } else {
                                                    "{v} Hz"
                                                }
                                            }
                                        }
                                    })
                            }
                        }
                    }
                    div { class: "flex gap-2 mt-3",
                        button {
                            class: "btn btn-sm btn-primary flex-1",
                            onclick: move |_| auto_save_restart(),
                            "Save playback settings"
                        }
                    }
                },
            }

            // ── Volume section ───────────────────────────────────────────────
            SettingsSection {
                title: "Volume Control",
                icon: "volume_up",
                content: rsx! {
                    div { class: "form-control mb-3",
                        label { class: "label",
                            span { class: "label-text font-medium", "Volume control type" }
                        }
                        select {
                            class: "select select-bordered select-sm w-full",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(v) = e.value().parse::<VolumeCrtlType>() {
                                    settings.write().volume_ctrl_settings.ctrl_device = v;
                                    auto_save_restart();
                                }
                            },
                            {
                                let current = settings.read().volume_ctrl_settings.ctrl_device;
                                VolumeCrtlType::iter()
                                    .map(move |vt| {
                                        let label: &'static str = vt.into();
                                        rsx! {
                                            option { value: "{label}", selected: current == vt, "{label}" }
                                        }
                                    })
                            }
                        }
                    }

                    // ALSA mixer selection
                    {
                        let card_id = settings.read().alsa_settings.output_device.card_id.clone();
                        let mixers = settings.read().alsa_settings.find_mixers_by_card_id(&card_id);
                        let ctrl = settings.read().volume_ctrl_settings.ctrl_device;
                        if ctrl == VolumeCrtlType::Alsa && !mixers.is_empty() {
                            rsx! {
                                div { class: "form-control mb-3",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "ALSA mixer" }
                                    }
                                    select {
                                        class: "select select-bordered select-sm w-full",
                                        onchange: move |e: Event<FormData>| {
                                            settings.write().volume_ctrl_settings.alsa_mixer_name = Some(e.value());
                                            auto_save_restart();
                                        },
                                        {
                                            let current = settings.read().volume_ctrl_settings.alsa_mixer_name.clone();
                                            mixers
                                                .into_iter()
                                                .map(move |m| {
                                                    let selected = current.as_deref() == Some(&m.name);
                                                    rsx! {
                                                        option { value: "{m.name}", selected, "{m.name}" }
                                                    }
                                                })
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }

                    NumberInput {
                        label: "Volume step",
                        value: settings.read().volume_ctrl_settings.volume_step.to_string(),
                        min: "1",
                        max: "50",
                        onchange: move |v: String| {
                            if let Ok(n) = v.parse::<u8>() {
                                settings.write().volume_ctrl_settings.volume_step = n;
                                auto_save();
                            }
                        },
                    }
                },
            }

            // ── VU Meter & Normalization ──────────────────────────────────────
            SettingsSection {
                title: "Visualization & Normalization",
                icon: "graphic_eq",
                content: rsx! {
                    ToggleRow {
                        label: "Enable Visualization",
                        checked: settings.read().rs_player_settings.vu_meter_enabled,
                        onchange: move |_| {
                            let v = !settings.read().rs_player_settings.vu_meter_enabled;
                            settings.write().rs_player_settings.vu_meter_enabled = v;
                            auto_save_restart();
                        },
                    }
                    div { class: "divider my-2" }
                    ToggleRow {
                        label: "Enable loudness normalization",
                        checked: settings.read().rs_player_settings.loudness_normalization_enabled,
                        onchange: move |_| {
                            let v = !settings.read().rs_player_settings.loudness_normalization_enabled;
                            settings.write().rs_player_settings.loudness_normalization_enabled = v;
                            auto_save_restart();
                        },
                    }
                    if settings.read().rs_player_settings.loudness_normalization_enabled {
                        div { class: "mt-3 space-y-3",
                            NumberInput {
                                label: "Target LUFS",
                                value: settings.read().rs_player_settings.loudness_normalization_target_lufs.to_string(),
                                min: "-40",
                                max: "0",
                                onchange: move |v: String| {
                                    if let Ok(n) = v.parse::<f64>() {
                                        settings.write().rs_player_settings.loudness_normalization_target_lufs = n;
                                        auto_save_restart();
                                    }
                                },
                            }
                            div { class: "form-control",
                                label { class: "label",
                                    span { class: "label-text font-medium", "Normalization source" }
                                }
                                select {
                                    class: "select select-bordered select-sm w-full",
                                    onchange: move |e: Event<FormData>| {
                                        let src = match e.value().as_str() {
                                            "Auto" => NormalizationSource::Auto,
                                            "FileTagsTrack" => NormalizationSource::FileTagsTrack,
                                            "FileTagsAlbum" => NormalizationSource::FileTagsAlbum,
                                            "Calculated" => NormalizationSource::Calculated,
                                            _ => NormalizationSource::Auto,
                                        };
                                        settings.write().rs_player_settings.loudness_normalization_source = src;
                                        auto_save_restart();
                                    },
                                    {
                                        let current = settings
                                            .read()
                                            .rs_player_settings
                                            .loudness_normalization_source
                                            .clone();
                                        [
                                            ("Auto", NormalizationSource::Auto),
                                            ("FileTagsTrack", NormalizationSource::FileTagsTrack),
                                            ("FileTagsAlbum", NormalizationSource::FileTagsAlbum),
                                            ("Calculated", NormalizationSource::Calculated),
                                        ]
                                            .into_iter()
                                            .map(move |(label, src)| {
                                                rsx! {
                                                    option { value: "{label}", selected: current == src, "{label}" }
                                                }
                                            })
                                    }
                                }
                            }
                        }
                    }
                },
            }

            // ── DSP section ───────────────────────────────────────────────────
            SettingsSection {
                title: "DSP Equalizer",
                icon: "tune",
                content: rsx! {
                    ToggleRow {
                        label: "Enable DSP",
                        checked: settings.read().rs_player_settings.dsp_settings.enabled,
                        onchange: move |_| {
                            let v = !settings.read().rs_player_settings.dsp_settings.enabled;
                            settings.write().rs_player_settings.dsp_settings.enabled = v;
                            *dsp_dirty.write() = true;
                            *pending_restart.write() = true;
                        },
                    }

                    if settings.read().rs_player_settings.dsp_settings.enabled {
                        div { class: "mt-3",
                            // Presets dropdown
                            div { class: "mb-3",
                                label { class: "label text-xs", "Load Preset" }
                                select {
                                    class: "select select-bordered select-sm w-full",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(idx) = e.value().parse::<usize>() {
                                            if let Some(preset) = get_dsp_presets().get(idx) {
                                                settings.write().rs_player_settings.dsp_settings.filters = preset
                                                    .filters
                                                    .clone();
                                                *dsp_dirty.write() = true;
                                            }
                                        }
                                    },
                                    option { value: "", "Select a preset..." }
                                    {
                                        get_dsp_presets()
                                            .iter()
                                            .enumerate()
                                            .map(|(i, preset)| {
                                                rsx! {
                                                    option { value: "{i}", "{preset.name}" }
                                                }
                                            })
                                    }
                                }
                            }

                            // Filter list
                            {
                                let filters: Vec<(usize, FilterConfig)> = settings
                                    .read()
                                    .rs_player_settings
                                    .dsp_settings
                                    .filters
                                    .iter()
                                    .cloned()
                                    .enumerate()
                                    .collect();
                                filters
                                    .into_iter()
                                    .map(|(i, fc)| {
                                        let ft = filter_type_of(&fc.filter);
                                        rsx! {
                                            div { class: "border border-base-300 rounded p-2 mb-2",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    select {
                                                        class: "select select-bordered select-xs flex-1",
                                                        onchange: move |e: Event<FormData>| {
                                                            let new_ft = DspFilterType::iter()
                                                                .find(|t| t.label() == e.value())
                                                                .unwrap_or(DspFilterType::Peaking);
                                                            if let Some(fc) = settings
                                                                .write()
                                                                .rs_player_settings
                                                                .dsp_settings
                                                                .filters
                                                                .get_mut(i)
                                                            {
                                                                fc.filter = new_ft.default_filter();
                                                            }
                                                            *dsp_dirty.write() = true;
                                                        },
                                                        {
                                                            DspFilterType::iter()
                                                                .map(move |t| {
                                                                    rsx! {
                                                                        option { value: "{t.label()}", selected: t == ft, "{t.label()}" }
                                                                    }
                                                                })
                                                        }
                                                    }
                                                    button {
                                                        class: "btn btn-ghost btn-xs text-error",
                                                        onclick: move |_| {
                                                            settings.write().rs_player_settings.dsp_settings.filters.remove(i);
                                                            *dsp_dirty.write() = true;
                                                        },
                                                        i { class: "material-icons text-sm", "delete" }
                                                    }
                                                }
                                                DspFilterFields {
                                                    filter: fc.filter.clone(),
                                                    index: i,
                                                    settings,
                                                    dsp_dirty,
                                                }
                                            }
                                        }
                                    })
                            }

                            // Add filter / clear / apply buttons
                            div { class: "flex gap-2 mt-2 flex-wrap",
                                button {
                                    class: "btn btn-sm btn-ghost",
                                    onclick: move |_| {
                                        settings
                                            .write()
                                            .rs_player_settings
                                            .dsp_settings
                                            .filters
                                            .push(FilterConfig {
                                                filter: DspFilter::Peaking {
                                                    freq: 1000.0,
                                                    gain: 0.0,
                                                    q: 0.707,
                                                },
                                                channels: vec![],
                                            });
                                        *dsp_dirty.write() = true;
                                    },
                                    i { class: "material-icons text-sm mr-1", "add" }
                                    "Add filter"
                                }
                                if !settings.read().rs_player_settings.dsp_settings.filters.is_empty() {
                                    button {
                                        class: "btn btn-sm btn-ghost text-error",
                                        onclick: move |_| *confirm.write() = Some(ConfirmAction::ClearDspFilters),
                                        i { class: "material-icons text-sm mr-1", "clear_all" }
                                        "Clear all"
                                    }
                                }
                                if dsp_dirty() {
                                    button {
                                        class: "btn btn-sm btn-primary",
                                        onclick: move |_| {
                                            let dsp = settings.read().rs_player_settings.dsp_settings.clone();
                                            ws_send(
                                                &ws,
                                                &UserCommand::UpdateDsp(DspSettings {
                                                    enabled: dsp.enabled,
                                                    filters: dsp.filters,
                                                }),
                                            );
                                            *dsp_dirty.write() = false;
                                            auto_save();
                                        },
                                        i { class: "material-icons text-sm mr-1", "check" }
                                        "Apply DSP"
                                    }
                                }
                            }
                        }
                    }
                },
            }

            // ── Music Library section ─────────────────────────────────────────
            SettingsSection {
                title: "Music Library",
                icon: "library_music",
                content: rsx! {
                    MusicLibraryContent {
                        settings,
                        confirm,
                        ws,
                        saving,
                    }
                },
            }

            // ── USB ───────────────────────────────────────────────────────────
            SettingsSection {
                title: "RSPlayer firmware control channel",
                icon: "usb",
                content: rsx! {
                    ToggleRow {
                        label: "Enable USB command channel",
                        checked: settings.read().usb_settings.enabled,
                        onchange: move |_| {
                            let v = !settings.read().usb_settings.enabled;
                            settings.write().usb_settings.enabled = v;
                            auto_save_restart();
                        },
                    }
                    if settings.read().usb_settings.enabled {
                        div { class: "flex flex-wrap gap-2 mt-3",
                            button {
                                class: "btn btn-sm btn-error w-fit",
                                onclick: move |_| send_system_cmd(&ws, SystemCommand::SetFirmwarePower(false)),
                                i { class: "material-icons text-sm mr-1", "power_off" }
                                "Power Off"
                            }
                            button {
                                class: "btn btn-sm btn-success w-fit",
                                onclick: move |_| send_system_cmd(&ws, SystemCommand::SetFirmwarePower(true)),
                                i { class: "material-icons text-sm mr-1", "power" }
                                "Power On"
                            }
                        }
                    }
                },
            }

            // ── Restart pending banner ────────────────────────────────────────
            if pending_restart() {
                div { class: "alert alert-warning shadow-sm",
                    i { class: "material-icons text-lg", "restart_alt" }
                    span { "Some changes require a player restart to take effect." }
                    button {
                        class: "btn btn-sm btn-warning ml-auto",
                        onclick: move |_| *confirm.write() = Some(ConfirmAction::RestartPlayer),
                        "Restart now"
                    }
                }
            }

            // ── System ────────────────────────────────────────────────────────
            SettingsSection {
                title: "System",
                icon: "settings_power",
                content: rsx! {
                    div { class: "flex flex-wrap gap-2",
                        button {
                            class: "btn btn-sm btn-warning w-fit",
                            onclick: move |_| *confirm.write() = Some(ConfirmAction::RestartPlayer),
                            i { class: "material-icons text-sm mr-1", "restart_alt" }
                            "Restart RSPlayer"
                        }
                        button {
                            class: "btn btn-sm btn-warning w-fit",
                            onclick: move |_| *confirm.write() = Some(ConfirmAction::RestartSystem),
                            i { class: "material-icons text-sm mr-1", "power_settings_new" }
                            "Restart system"
                        }
                        button {
                            class: "btn btn-sm btn-error w-fit",
                            onclick: move |_| *confirm.write() = Some(ConfirmAction::ShutdownSystem),
                            i { class: "material-icons text-sm mr-1", "power_off" }
                            "Shutdown"
                        }
                    }
                    p { class: "text-xs text-base-content/40 mt-2", "Version: {settings.read().version}" }
                },
            }
        
        }

        // ── Confirm dialog ────────────────────────────────────────────────────
        if confirm.read().is_some() {
            div { class: "modal modal-open",
                div { class: "modal-box",
                    h3 { class: "font-bold text-lg", "Confirm" }
                    p { class: "py-4",
                        match confirm.read().as_ref() {
                            Some(ConfirmAction::FullRescan) => {
                                "Perform a full rescan of the music library? This may take a while."
                            }
                            Some(ConfirmAction::RestartPlayer) => "Restart RSPlayer service?",
                            Some(ConfirmAction::RestartSystem) => "Restart the system?",
                            Some(ConfirmAction::ShutdownSystem) => "Shut down the system?",
                            Some(ConfirmAction::RemoveMusicDirectory(_)) => "Remove this music directory?",
                            Some(ConfirmAction::RemoveNetworkMount(_)) => "Remove this network mount?",
                            Some(ConfirmAction::ClearDspFilters) => "Remove all DSP filters?",
                            None => "",
                        }
                    }
                    div { class: "modal-action",
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| *confirm.write() = None,
                            "Cancel"
                        }
                        button {
                            class: "btn btn-sm btn-error",
                            onclick: move |_| do_confirm(),
                            "Confirm"
                        }
                    }
                }
            }
        }
    }
}

// ─── Music Library component ──────────────────────────────────────────────────
// Owns its own signal subscriptions so that add/remove immediately re-renders this
// component without waiting for the parent to propagate new content props.

#[component]
fn MusicLibraryContent(
    settings: Signal<Settings>,
    confirm: Signal<Option<ConfirmAction>>,
    ws: Signal<Option<WebSocket>>,
    saving: Signal<bool>,
) -> Element {
    let state = use_context::<AppState>();

    let mut mf_name = use_signal(String::new);
    let mut mf_type = use_signal(|| NetworkMountType::Nfs);
    let mut mf_server = use_signal(String::new);
    let mut mf_share = use_signal(String::new);
    let mut mf_username = use_signal(String::new);
    let mut mf_password = use_signal(String::new);
    let mut mf_domain = use_signal(String::new);
    let mut network_mounts_open = use_signal(|| false);
    let mut new_dir = use_signal(String::new);

    let mount_statuses = state.mount_statuses.read().clone();
    let music_dir_statuses = state.music_dir_statuses.read().clone();
    let external_mounts = state.external_mounts.read().clone();

    let mut auto_save = move || {
        *saving.write() = true;
        let s = settings.read().clone();
        spawn(async move {
            let _ = Request::post(API_SETTINGS_PATH)
                .json(&s)
                .expect("serialize settings")
                .send()
                .await;
            *saving.write() = false;
        });
    };

    rsx! {
        // Existing network mounts with status badge
        {
            let mounts = settings.read().network_storage_settings.mounts.clone();
            mounts
                .into_iter()
                .map(|m| {
                    let name = m.name.clone();
                    let name2 = m.name.clone();
                    let name3 = m.name.clone();
                    let status = mount_statuses.iter().find(|s| s.name == m.name).cloned();
                    let mount_point = m
                        .mount_point
                        .clone()
                        .unwrap_or_else(|| format!("/mnt/rsplayer/{}", m.name));
                    let type_label = match m.mount_type {
                        NetworkMountType::Smb => "SMB",
                        NetworkMountType::Nfs => "NFS",
                    };
                    let source = match m.mount_type {
                        NetworkMountType::Smb => format!("//{}/{}", m.server, m.share),
                        NetworkMountType::Nfs => format!("{}:{}", m.server, m.share),
                    };
                    let is_mounted = status.as_ref().map_or(false, |s| s.is_mounted);
                    let (status_label, status_class) = match status.as_ref() {
                        Some(s) if s.readable && s.writable => {
                            ("Read / Write", "badge-success")
                        }
                        Some(s) if s.readable => ("Read only", "badge-warning"),
                        Some(s) if s.is_mounted => ("Not accessible", "badge-error"),
                        Some(_) => ("Not mounted", "badge-error"),
                        None => ("Unknown", "badge-ghost"),
                    };
                    rsx! {
                        div { class: "flex items-center gap-2 py-1.5 border-b border-base-300",
                            span { class: "badge badge-sm {status_class} shrink-0", "{status_label}" }
                            div { class: "flex-1 min-w-0",
                                p { class: "text-sm font-medium truncate", "{m.name} ({type_label})" }
                                p { class: "text-xs text-base-content/50 truncate", "{source} → {mount_point}" }
                            }
                            if is_mounted {
                                button {
                                    class: "btn btn-warning btn-xs",
                                    onclick: move |_| ws_send(&ws, &UserCommand::Storage(StorageCommand::Unmount(name.clone()))),
                                    "Unmount"
                                }
                            } else {
                                button {
                                    class: "btn btn-success btn-xs",
                                    onclick: move |_| {
                                        let cfg = NetworkMountConfig {
                                            name: name2.clone(),
                                            mount_type: m.mount_type.clone(),
                                            server: m.server.clone(),
                                            share: m.share.clone(),
                                            username: m.username.clone(),
                                            password: m.password.clone(),
                                            domain: m.domain.clone(),
                                            mount_point: m.mount_point.clone(),
                                        };
                                        ws_send(&ws, &UserCommand::Storage(StorageCommand::Mount(cfg)));
                                    },
                                    "Mount"
                                }
                            }
                            button {
                                class: "btn btn-ghost btn-xs text-error",
                                onclick: move |_| *confirm.write() = Some(ConfirmAction::RemoveNetworkMount(name3.clone())),
                                i { class: "material-icons text-sm", "delete" }
                            }
                        }
                    }
                })
        }

        // Local directories (excluding network mount points) with status badge
        {
            let mount_points: Vec<String> = settings
                .read()
                .network_storage_settings
                .mounts
                .iter()
                .map(|m| {
                    m
                        .mount_point
                        .clone()
                        .unwrap_or_else(|| format!("/mnt/rsplayer/{}", m.name))
                })
                .collect();
            let dirs: Vec<(usize, String)> = settings
                .read()
                .metadata_settings
                .effective_directories()
                .into_iter()
                .enumerate()
                .filter(|(_, dir)| !mount_points.contains(dir))
                .collect();
            dirs.into_iter()
                .map(|(i, dir)| {
                    let dir_status = music_dir_statuses
                        .iter()
                        .find(|s| s.path == dir)
                        .cloned();
                    let (status_label, status_class) = match dir_status.as_ref() {
                        Some(s) if s.readable && s.writable => {
                            ("Read / Write", "badge-success")
                        }
                        Some(s) if s.readable => ("Read only", "badge-warning"),
                        Some(_) => ("Not accessible", "badge-error"),
                        None => ("Unknown", "badge-ghost"),
                    };
                    rsx! {
                        div { class: "flex items-center gap-2 py-1.5 border-b border-base-300",
                            span { class: "badge badge-sm {status_class} shrink-0", "{status_label}" }
                            div { class: "flex-1 min-w-0",
                                p { class: "text-sm font-medium truncate", "{dir}" }
                                p { class: "text-xs text-base-content/50", "Local directory" }
                            }
                            button {
                                class: "btn btn-ghost btn-xs text-error",
                                onclick: move |_| *confirm.write() = Some(ConfirmAction::RemoveMusicDirectory(i)),
                                i { class: "material-icons text-sm", "delete" }
                            }
                        }
                    }
                })
        }

        // Add Local Directory
        div { class: "mt-3 p-3 bg-base-200 rounded",
            p { class: "text-sm font-medium mb-2", "Add Local Directory" }
            div { class: "flex gap-2",
                input {
                    class: "input input-sm input-bordered flex-1",
                    r#type: "text",
                    placeholder: "/path/to/music",
                    oninput: move |e| new_dir.set(e.value()),
                    value: "{new_dir}",
                }
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |_| {
                        let dir = new_dir();
                        if !dir.is_empty() {
                            settings.write().metadata_settings.music_directories.push(dir);
                            new_dir.set(String::new());
                            auto_save();
                        }
                    },
                    "Add"
                }
            }
        }

        // Collapsible Network Mounts subsection
        div { class: "mt-3",
            div {
                class: "flex items-center justify-between cursor-pointer select-none py-2 border-b border-base-300",
                onclick: move |_| {
                    let v = !*network_mounts_open.read();
                    *network_mounts_open.write() = v;
                },
                span { class: "text-sm font-medium", "Network Mounts" }
                i {
                    class: "material-icons text-sm transition-transform",
                    style: if *network_mounts_open.read() { "transform: rotate(180deg)" } else { "" },
                    "expand_more"
                }
            }
            if *network_mounts_open.read() {
                div { class: "mt-3 space-y-2",
                    p { class: "text-sm font-medium", "Add Network Mount" }
                    div { class: "flex flex-wrap gap-2",
                        div { class: "flex flex-col gap-1",
                            label { class: "text-xs text-base-content/60", "Type" }
                            select {
                                class: "select select-bordered select-sm",
                                onchange: move |e: Event<FormData>| {
                                    *mf_type.write() = match e.value().as_str() {
                                        "Nfs" => NetworkMountType::Nfs,
                                        _ => NetworkMountType::Smb,
                                    };
                                },
                                option {
                                    value: "Nfs",
                                    selected: *mf_type.read() == NetworkMountType::Nfs,
                                    "NFS"
                                }
                                option {
                                    value: "Smb",
                                    selected: *mf_type.read() == NetworkMountType::Smb,
                                    "SMB/CIFS"
                                }
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            label { class: "text-xs text-base-content/60", "Server *" }
                            input {
                                class: "input input-sm input-bordered",
                                placeholder: "192.168.1.100",
                                value: "{mf_server}",
                                oninput: move |e| mf_server.set(e.value()),
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            label { class: "text-xs text-base-content/60", "Share (remote dir) *" }
                            input {
                                class: "input input-sm input-bordered",
                                placeholder: "music",
                                value: "{mf_share}",
                                oninput: move |e| mf_share.set(e.value()),
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            label { class: "text-xs text-base-content/60", "Name - local dir (optional)" }
                            input {
                                class: "input input-sm input-bordered",
                                placeholder: "auto from share",
                                value: "{mf_name}",
                                oninput: move |e| mf_name.set(e.value()),
                            }
                        }
                    
                    }
                    if *mf_type.read() == NetworkMountType::Smb {
                        div { class: "flex flex-wrap gap-2",
                            div { class: "flex flex-col gap-1",
                                label { class: "text-xs text-base-content/60", "Username (optional)" }
                                input {
                                    class: "input input-sm input-bordered",
                                    placeholder: "guest",
                                    value: "{mf_username}",
                                    oninput: move |e| mf_username.set(e.value()),
                                }
                            }
                            div { class: "flex flex-col gap-1",
                                label { class: "text-xs text-base-content/60", "Password (optional)" }
                                input {
                                    class: "input input-sm input-bordered",
                                    r#type: "password",
                                    value: "{mf_password}",
                                    oninput: move |e| mf_password.set(e.value()),
                                }
                            }
                            div { class: "flex flex-col gap-1",
                                label { class: "text-xs text-base-content/60", "Domain (optional)" }
                                input {
                                    class: "input input-sm input-bordered",
                                    placeholder: "WORKGROUP",
                                    value: "{mf_domain}",
                                    oninput: move |e| mf_domain.set(e.value()),
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| {
                            let name = {
                                let n = mf_name();
                                let raw = if n.is_empty() { mf_share() } else { n };
                                raw.replace('/', "_")
                            };
                            let cfg = NetworkMountConfig {
                                name: name.clone(),
                                mount_type: mf_type.read().clone(),
                                server: mf_server(),
                                share: mf_share(),
                                username: if mf_username().is_empty() { None } else { Some(mf_username()) },
                                password: if mf_password().is_empty() { None } else { Some(mf_password()) },
                                domain: if mf_domain().is_empty() { None } else { Some(mf_domain()) },
                                mount_point: None,
                            };
                            let mount_point = format!("/mnt/rsplayer/{}", name);
                            {
                                let mut s = settings.write();
                                s.network_storage_settings.mounts.push(cfg.clone());
                                if !s.metadata_settings.music_directories.contains(&mount_point) {
                                    s.metadata_settings.music_directories.push(mount_point);
                                }
                            }
                            ws_send(&ws, &UserCommand::Storage(StorageCommand::Mount(cfg)));
                            auto_save();
                            mf_name.set(String::new());
                            mf_server.set(String::new());
                            mf_share.set(String::new());
                            mf_username.set(String::new());
                            mf_password.set(String::new());
                            mf_domain.set(String::new());
                        },
                        "Mount"
                    }
                    div { class: "alert alert-info py-2 text-xs",
                        i { class: "material-icons text-sm mr-1", "info" }
                        span {
                            "Mounts are created under /mnt/rsplayer/<name> and automatically added as music directories."
                        }
                    }
                }
            }
        }

        // Detected external network mounts
        if !external_mounts.is_empty() {
            div { class: "mt-3 p-3 rounded border border-info bg-info/10",
                p { class: "text-sm font-medium mb-1", "Detected External Network Mounts" }
                p { class: "text-xs text-base-content/60 mb-3",
                    "These network mounts were found on the system but are not managed by rsplayer. Click Save to add them as music sources."
                }
                {
                    external_mounts
                        .iter()
                        .map(|em| {
                            let mp = em.mount_point.clone();
                            let (status_label, status_class) = match (em.readable, em.writable) {
                                (true, true) => ("Read / Write", "badge-success"),
                                (true, false) => ("Read only", "badge-warning"),
                                _ => ("Not accessible", "badge-error"),
                            };
                            rsx! {
                                div { class: "flex items-center gap-2 py-1",
                                    span { class: "badge badge-sm {status_class} shrink-0", "{status_label}" }
                                    div { class: "flex-1 min-w-0",
                                        p { class: "text-sm font-medium truncate", "{em.source}" }
                                        p { class: "text-xs text-base-content/50 truncate", "{em.mount_point}" }
                                    }
                                    button {
                                        class: "btn btn-sm btn-primary",
                                        onclick: move |_| ws_send(
                                            &ws,
                                            &UserCommand::Storage(StorageCommand::SaveExternalMount(mp.clone())),
                                        ),
                                        "Save"
                                    }
                                }
                            }
                        })
                }
            }
        }

        ToggleRow {
            label: "Follow symlinks",
            checked: settings.read().metadata_settings.follow_links,
            onchange: move |_| {
                let v = !settings.read().metadata_settings.follow_links;
                settings.write().metadata_settings.follow_links = v;
                auto_save();
            },
        }

        // Scan status message
        {
            let msg = state.metadata_scan_msg.read().clone();
            if let Some(m) = msg {
                rsx! {
                    p { class: "text-xs text-base-content/60 mt-2 truncate", "{m}" }
                }
            } else {
                rsx! {}
            }
        }

        div { class: "flex gap-2 mt-3 justify-end",
            button {
                class: "btn btn-sm",
                onclick: move |_| {
                    ws_send(
                        &ws,
                        &UserCommand::Metadata(MetadataCommand::RescanMetadata(String::new(), false)),
                    );
                },
                "Update library"
            }
            button {
                class: "btn btn-sm btn-warning",
                onclick: move |_| *confirm.write() = Some(ConfirmAction::FullRescan),
                "Full rescan"
            }
        }
    }
}

// ─── Sub-components ───────────────────────────────────────────────────────────

#[component]
fn SettingsSection(title: &'static str, icon: &'static str, content: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "border border-base-300 rounded-lg overflow-hidden",
            button {
                class: "w-full flex items-center gap-3 px-4 py-3 text-left bg-base-200 hover:bg-base-300 transition-colors",
                onclick: move |_| open.toggle(),
                i { class: "material-icons text-base text-primary", "{icon}" }
                span { class: "flex-1 font-medium", "{title}" }
                i { class: if open() { "material-icons text-base transition-transform rotate-180" } else { "material-icons text-base transition-transform" },
                    "expand_more"
                }
            }
            if open() {
                div { class: "px-4 py-3", {content} }
            }
        }
    }
}

#[component]
fn ToggleRow(label: &'static str, checked: bool, onchange: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "flex items-center justify-between py-1.5",
            span { class: "text-sm", "{label}" }
            input {
                r#type: "checkbox",
                class: "toggle toggle-sm toggle-primary",
                checked,
                onclick: move |e| onchange.call(e),
            }
        }
    }
}

#[component]
fn NumberInput(
    label: &'static str,
    value: String,
    min: &'static str,
    max: &'static str,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "form-control mb-2",
            label { class: "label py-0.5",
                span { class: "label-text text-sm", "{label}" }
            }
            input {
                r#type: "number",
                class: "input input-sm input-bordered w-full",
                value,
                min,
                max,
                onchange: move |e| onchange.call(e.value()),
            }
        }
    }
}

#[component]
fn DspFilterFields(
    filter: DspFilter,
    index: usize,
    mut settings: Signal<Settings>,
    mut dsp_dirty: Signal<bool>,
) -> Element {
    let mut update = move |field: &'static str, val: String| {
        if let Ok(v) = val.parse::<f64>() {
            if let Some(fc) = settings.write().rs_player_settings.dsp_settings.filters.get_mut(index) {
                match (&mut fc.filter, field) {
                    (DspFilter::Peaking { freq, .. }, "freq") => *freq = v,
                    (DspFilter::Peaking { gain, .. }, "gain") => *gain = v,
                    (DspFilter::Peaking { q, .. }, "q") => *q = v,
                    (DspFilter::LowShelf { freq, .. }, "freq") => *freq = v,
                    (DspFilter::LowShelf { gain, .. }, "gain") => *gain = v,
                    (DspFilter::LowShelf { q, .. }, "q") => *q = Some(v),
                    (DspFilter::HighShelf { freq, .. }, "freq") => *freq = v,
                    (DspFilter::HighShelf { gain, .. }, "gain") => *gain = v,
                    (DspFilter::HighShelf { q, .. }, "q") => *q = Some(v),
                    (DspFilter::LowPass { freq, .. }, "freq") => *freq = v,
                    (DspFilter::LowPass { q, .. }, "q") => *q = v,
                    (DspFilter::HighPass { freq, .. }, "freq") => *freq = v,
                    (DspFilter::HighPass { q, .. }, "q") => *q = v,
                    (DspFilter::BandPass { freq, .. }, "freq") => *freq = v,
                    (DspFilter::BandPass { q, .. }, "q") => *q = v,
                    (DspFilter::Notch { freq, .. }, "freq") => *freq = v,
                    (DspFilter::Notch { q, .. }, "q") => *q = v,
                    (DspFilter::AllPass { freq, .. }, "freq") => *freq = v,
                    (DspFilter::AllPass { q, .. }, "q") => *q = v,
                    (DspFilter::LowPassFO { freq }, "freq") => *freq = v,
                    (DspFilter::HighPassFO { freq }, "freq") => *freq = v,
                    (DspFilter::LowShelfFO { freq, .. }, "freq") => *freq = v,
                    (DspFilter::LowShelfFO { gain, .. }, "gain") => *gain = v,
                    (DspFilter::HighShelfFO { freq, .. }, "freq") => *freq = v,
                    (DspFilter::HighShelfFO { gain, .. }, "gain") => *gain = v,
                    (DspFilter::Gain { gain }, "gain") => *gain = v,
                    _ => {}
                }
            }
            *dsp_dirty.write() = true;
        }
    };

    let fields: Vec<(&'static str, String)> = match &filter {
        DspFilter::Peaking { freq, gain, q } => vec![
            ("freq", format!("{freq}")),
            ("gain", format!("{gain}")),
            ("q", format!("{q}")),
        ],
        DspFilter::LowShelf { freq, gain, q, .. } | DspFilter::HighShelf { freq, gain, q, .. } => vec![
            ("freq", format!("{freq}")),
            ("gain", format!("{gain}")),
            ("q", format!("{}", q.unwrap_or(0.707))),
        ],
        DspFilter::LowPass { freq, q }
        | DspFilter::HighPass { freq, q }
        | DspFilter::BandPass { freq, q }
        | DspFilter::Notch { freq, q }
        | DspFilter::AllPass { freq, q } => vec![("freq", format!("{freq}")), ("q", format!("{q}"))],
        DspFilter::LowPassFO { freq } | DspFilter::HighPassFO { freq } => vec![("freq", format!("{freq}"))],
        DspFilter::LowShelfFO { freq, gain } | DspFilter::HighShelfFO { freq, gain } => {
            vec![("freq", format!("{freq}")), ("gain", format!("{gain}"))]
        }
        DspFilter::Gain { gain } => vec![("gain", format!("{gain}"))],
        DspFilter::LinkwitzTransform {
            freq_act,
            q_act,
            freq_target,
            q_target,
        } => vec![
            ("freq", format!("{freq_act}")),
            ("q", format!("{q_act}")),
            ("freq", format!("{freq_target}")),
            ("q", format!("{q_target}")),
        ],
    };

    rsx! {
        div { class: "grid grid-cols-3 gap-1",
            {
                fields
                    .into_iter()
                    .map(|(field, val)| {
                        rsx! {
                            div { class: "form-control",
                                label { class: "label py-0",
                                    span { class: "label-text text-xs", "{field}" }
                                }
                                input {
                                    r#type: "number",
                                    class: "input input-xs input-bordered w-full",
                                    value: "{val}",
                                    step: "0.001",
                                    onchange: move |e| update(field, e.value()),
                                }
                            }
                        }
                    })
            }
        }
    }
}

// ─── Appearance / Theme picker ────────────────────────────────────────────────

/// (id, display label)
const THEMES: &[(&str, &str)] = &[
    ("dark", "Dark"),
    ("light", "Light"),
    ("synthwave", "Synthwave"),
    ("dracula", "Dracula"),
    ("nord", "Nord"),
    ("dim", "Dim"),
    ("aqua", "Aqua"),
    ("coffee", "Coffee"),
    ("caramellatte", "Caramel"),
    ("black", "Black"),
];

#[component]
fn AppearanceSection() -> Element {
    let mut state = use_context::<AppState>();
    let current = state.current_theme.read().clone();
    let show_bg = *state.show_bg_image.read();

    rsx! {
        div { class: "flex items-center justify-between py-1.5 mb-3",
            span { class: "text-sm", "Album art background" }
            input {
                r#type: "checkbox",
                class: "toggle toggle-sm toggle-primary",
                checked: show_bg,
                onclick: move |_| {
                    let next = !*state.show_bg_image.peek();
                    *state.show_bg_image.write() = next;
                    if let Some(window) = web_sys::window() {
                        if let Ok(Some(storage)) = window.local_storage() {
                            let _ = storage
                                .set_item(
                                    "rsplayer_show_bg_image",
                                    if next { "true" } else { "false" },
                                );
                        }
                    }
                },
            }
        }
        div { class: "flex flex-wrap gap-3",
            {
                THEMES
                    .iter()
                    .map(|(id, label)| {
                        let id = *id;
                        let label = *label;
                        let active = current == id;
                        rsx! {
                            button {
                                key: "{id}",
                                class: if active { "flex flex-col items-center gap-1 p-1 rounded-lg border-2 border-primary cursor-pointer" } else { "flex flex-col items-center gap-1 p-1 rounded-lg border-2 border-transparent cursor-pointer hover:border-base-content/20" }, // Swatch preview — rendered in its own data-theme context so colors
                                onclick: move |_| {
                                    *state.current_theme.write() = id.to_string();
                                    if let Some(window) = web_sys::window() {
                                        if let Ok(Some(storage)) = window.local_storage() {
                                            let _ = storage.set_item("rsplayer_theme", id);
                                        }
                                    }
                                },
                                // Swatch preview — rendered in its own data-theme context so colors
                                // reflect that theme regardless of the active page theme.
                                div {
                                    "data-theme": id,
                                    class: "w-14 h-10 rounded overflow-hidden grid grid-cols-2",
                                    div { class: "bg-base-100 col-span-1 row-span-2" }
                                    div { class: "bg-primary" }
                                    div { class: "bg-secondary" }
                                }
                                span { class: "text-xs font-medium", "{label}" }
                            }
                        }
                    })
            }
        }
    }
}
