
use api_models::{
    common::{MetadataCommand::RescanMetadata, StorageCommand, SystemCommand, UserCommand, VolumeCrtlType},
    settings::{DspFilter, FilterConfig, MetadataStoreSettings, NetworkMountConfig, NetworkMountType, RsPlayerSettings, Settings},
    state::{ExternalMount, MountStatus, MusicDirStatus},
    validator::Validate,
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use gloo_console::{error, log};
use gloo_file::futures::read_as_text;
use gloo_file::File;
use gloo_net::{http::Request, Error};
use seed::{a, attrs, button, code, details, div, empty, footer, h1, header, i, input, label, option, p, prelude::*, section, select, span, style, summary, C, IF};
use wasm_bindgen::JsCast;
use web_sys::FileList;

use crate::dsp::get_dsp_presets;
use crate::view_spinner_modal;

const API_SETTINGS_PATH: &str = "/api/settings";

// ------ ------
//     Model

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    FullRescan,
    RestartPlayer,
    SaveAndRestartPlayer,
    RemoveMusicDirectory(usize),
    RemoveNetworkMount(String),
}

#[derive(Debug)]
pub struct Model {
    settings: Settings,
    selected_audio_card_id: String,
    waiting_response: bool,
    confirm_action: Option<ConfirmAction>,
    // Network mount form
    mount_form_name: String,
    mount_form_type: NetworkMountType,
    mount_form_server: String,
    mount_form_share: String,
    mount_form_username: String,
    mount_form_password: String,
    mount_form_domain: String,
    mount_statuses: Vec<MountStatus>,
    music_dir_statuses: Vec<MusicDirStatus>,
    new_music_dir: String,
    external_mounts: Vec<ExternalMount>,
    network_mounts_open: bool,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    // ---- on off toggles ----
    ToggleUsbEnabled,
    ToggleResumePlayback,
    ToggleRspAlsaBufferSize,
    // ---- Input capture ----
    InputNewMusicDir(String),
    AddMusicDirectory,
    RemoveMusicDirectory(usize),
    InputAlsaCardChange(String),
    InputAlsaPcmChange(String),
    InputVolumeStepChanged(String),

    InputRspInputBufferSizeChange(String),
    InputRspAudioBufferSizeChange(String),
    InputRspAlsaBufferSizeChange(String),
    InputRspThreadPriorityChange(String),
    InputRspFixedSampleRateChange(String),
    InputVolumeAlsaMixerChanged(String),
    ClickRescanMetadataButton(bool),

    InputAlsaDeviceChanged(String),

        // --- DSP ---
        ToggleDspEnabled,
        ToggleVuMeterEnabled,
        ToggleLoudnessNormalization,
        InputNormalizationTargetLufs(String),
        DspAddFilter,
        DspRemoveAllFilters,
        DspRemoveFilter(usize),
        DspUpdateFilterType(usize, FilterType),
        DspUpdateFilterValue(usize, DspField, String),
        DspUpdateFilterChannels(usize, String),
        DspApplySettings,
        DspLoadPreset(usize),
        DspImportConfig(FileList),
        DspConfigLoaded(String),

    // --- Confirm dialog ---
    ShowConfirm(ConfirmAction),
    ConfirmAccepted,
    ConfirmCancelled,

    // --- Buttons ----
    SaveSettingsAndRestart,
    SettingsSaved(Result<String, Error>),

    SettingsFetched(Settings),
    SendSystemCommand(SystemCommand),
    SendUserCommand(UserCommand),

    // --- Network Storage ---
    InputMountName(String),
    InputMountType(String),
    InputMountServer(String),
    InputMountShare(String),
    InputMountUsername(String),
    InputMountPassword(String),
    InputMountDomain(String),
    MountAdd,
    MountRemove(String),
    MountShare(String),
    UnmountShare(String),
    MountStatusReceived(Vec<MountStatus>),
    MusicDirStatusReceived(Vec<MusicDirStatus>),
    ExternalMountsReceived(Vec<ExternalMount>),
    SaveExternalMount(String),
    ToggleNetworkMounts,

    // --- Appearance ---
    /// Emitted when the user picks a theme in the settings page.
    /// The parent (lib.rs) intercepts this and calls applyTheme().
    SelectTheme(String),
}

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
pub enum FilterType {
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
    // LinkwitzTransform is complex, maybe skip for now in simple UI or add later
}

impl FilterType {
    fn to_string(&self) -> &str {
        match self {
            FilterType::Peaking => "Peaking",
            FilterType::LowShelf => "LowShelf",
            FilterType::HighShelf => "HighShelf",
            FilterType::LowPass => "LowPass",
            FilterType::HighPass => "HighPass",
            FilterType::BandPass => "BandPass",
            FilterType::Notch => "Notch",
            FilterType::AllPass => "AllPass",
            FilterType::LowPassFO => "LowPassFO",
            FilterType::HighPassFO => "HighPassFO",
            FilterType::LowShelfFO => "LowShelfFO",
            FilterType::HighShelfFO => "HighShelfFO",
            FilterType::Gain => "Gain",
        }
    }
}

#[derive(Debug, Clone)]
pub enum DspField {
    Freq,
    Gain,
    Q,
    Slope,
}

// ------ ------
//     Init
// ------ ------
#[allow(clippy::needless_pass_by_value)]
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    log!("Settings Init called");
    orders.perform_cmd(async {
        let response = Request::get(API_SETTINGS_PATH)
            .send()
            .await
            .expect("Failed to get settings from backend");

        let sett = response
            .json::<Settings>()
            .await
            .expect("failed to deserialize to Configuration");
        Msg::SettingsFetched(sett)
    });
    Model {
        settings: Settings::default(),
        selected_audio_card_id: String::new(),
        waiting_response: true,
        confirm_action: None,
        mount_form_name: String::new(),
        mount_form_type: NetworkMountType::Smb,
        mount_form_server: String::new(),
        mount_form_share: String::new(),
        mount_form_username: String::new(),
        mount_form_password: String::new(),
        mount_form_domain: String::new(),
        mount_statuses: Vec::new(),
        music_dir_statuses: Vec::new(),
        new_music_dir: String::new(),
        external_mounts: Vec::new(),
        network_mounts_open: false,
    }
}

// ------ ------
//    Update
// ------ ------
#[allow(clippy::too_many_lines)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::ShowConfirm(action) => {
            model.confirm_action = Some(action);
        }
        Msg::ConfirmCancelled => {
            model.confirm_action = None;
        }
        Msg::ConfirmAccepted => {
            if let Some(action) = model.confirm_action.take() {
                match action {
                    ConfirmAction::FullRescan => {
                        orders.send_msg(Msg::ClickRescanMetadataButton(true));
                    }
                    ConfirmAction::RestartPlayer => {
                        orders.send_msg(Msg::SendSystemCommand(SystemCommand::RestartRSPlayer));
                    }
                    ConfirmAction::SaveAndRestartPlayer => {
                        orders.send_msg(Msg::SaveSettingsAndRestart);
                    }
                    ConfirmAction::RemoveMusicDirectory(idx) => {
                        orders.send_msg(Msg::RemoveMusicDirectory(idx));
                    }
                    ConfirmAction::RemoveNetworkMount(name) => {
                        orders.send_msg(Msg::MountRemove(name));
                    }
                }
            }
        }
        Msg::SaveSettingsAndRestart => {
            match model.settings.validate() {
                Ok(_) => {
                    let settings = model.settings.clone();
                    orders.perform_cmd(async {
                        Msg::SettingsSaved(save_settings(settings, "reload=true".to_string()).await)
                    });
                    model.waiting_response = true;
                }
                Err(e) => {
                    error!(format!("Settings validation failed: {:?}", e));
                }
            }
        }
        Msg::ToggleUsbEnabled => {
            model.settings.usb_settings.enabled = !model.settings.usb_settings.enabled;
        }

        Msg::ToggleResumePlayback => {
            model.settings.auto_resume_playback = !model.settings.auto_resume_playback;
        }
        Msg::ToggleRspAlsaBufferSize => {
            if model.settings.rs_player_settings.alsa_buffer_size.is_some() {
                model.settings.rs_player_settings.alsa_buffer_size = None;
            } else {
                model.settings.rs_player_settings.alsa_buffer_size = Some(10000);
            }
        }

        Msg::InputNewMusicDir(value) => {
            model.new_music_dir = value;
        }
        Msg::AddMusicDirectory => {
            let dir = model.new_music_dir.trim().to_string();
            if !dir.is_empty() && !model.settings.metadata_settings.music_directories.contains(&dir) {
                model.settings.metadata_settings.music_directories.push(dir);
                model.new_music_dir.clear();
                let settings = model.settings.clone();
                orders.perform_cmd(async move {
                    _ = save_settings(settings, "reload=false".to_string()).await;
                    Msg::SendUserCommand(UserCommand::Storage(StorageCommand::QueryMusicDirStatus))
                });
            }
        }
        Msg::RemoveMusicDirectory(idx) => {
            if idx < model.settings.metadata_settings.music_directories.len() {
                model.settings.metadata_settings.music_directories.remove(idx);
                let settings = model.settings.clone();
                orders.perform_cmd(async move {
                    _ = save_settings(settings, "reload=false".to_string()).await;
                });
            }
        }

        Msg::InputAlsaCardChange(value) => {
            if value == "browser" {
                model.settings.local_browser_playback = true;
            } else {
                model.settings.local_browser_playback = false;
                model.selected_audio_card_id = value.clone();
                model.settings.alsa_settings.output_device.card_id = value.clone();
                if value == "pipewire" {
                    model.settings.volume_ctrl_settings.ctrl_device = VolumeCrtlType::Pipewire;
                } else {
                    model.settings.volume_ctrl_settings.ctrl_device = VolumeCrtlType::Alsa;
                }
            }
        }
        Msg::InputAlsaPcmChange(value) => {
            model
                .settings
                .alsa_settings
                .set_output_device(&model.selected_audio_card_id, &value);
        }

        Msg::InputVolumeStepChanged(step) => {
            model.settings.volume_ctrl_settings.volume_step = step.parse::<u8>().unwrap_or_default();
        }
        Msg::InputVolumeAlsaMixerChanged(mixer_name) => {
            model.settings.volume_ctrl_settings.alsa_mixer_name = Some(mixer_name);
        }

        Msg::InputRspInputBufferSizeChange(value) => {
            if let Ok(num) = value.parse::<usize>() {
                model.settings.rs_player_settings.input_stream_buffer_size_mb = num;
            };
        }
        Msg::InputRspAudioBufferSizeChange(value) => {
            if let Ok(num) = value.parse::<usize>() {
                model.settings.rs_player_settings.ring_buffer_size_ms = num;
            };
        }
        Msg::InputRspAlsaBufferSizeChange(value) => {
            if let Ok(num) = value.parse::<u32>() {
                model.settings.rs_player_settings.alsa_buffer_size = Some(num);
            };
        }
        Msg::InputRspThreadPriorityChange(value) => {
            if let Ok(num) = value.parse::<u8>() {
                if num > 0 && num < 100 {
                    model.settings.rs_player_settings.player_threads_priority = num;
                }
            };
        }
        Msg::InputRspFixedSampleRateChange(value) => {
            model.settings.rs_player_settings.fixed_output_sample_rate = value.parse::<u32>().ok().filter(|&r| r > 0);
        }
        Msg::SettingsFetched(sett) => {
            model.waiting_response = false;
            model.selected_audio_card_id = sett.alsa_settings.output_device.card_id.clone();
            model.settings = sett;
            // Query mount status on load
            orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(StorageCommand::QueryMountStatus)));
        }
        Msg::SettingsSaved(_saved) => {
            model.waiting_response = false;
        }
        Msg::ClickRescanMetadataButton(full_scan) => {
            let settings = model.settings.clone();
            orders.perform_cmd(async move {
                _ = save_settings(settings, "reload=false".to_string()).await;
            });
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(RescanMetadata(
                String::new(),
                full_scan,
            ))));
        }
        Msg::ToggleDspEnabled => {
            model.settings.rs_player_settings.dsp_settings.enabled = !model.settings.rs_player_settings.dsp_settings.enabled;
        }
        Msg::ToggleVuMeterEnabled => {
            model.settings.rs_player_settings.vu_meter_enabled = !model.settings.rs_player_settings.vu_meter_enabled;
        }
        Msg::ToggleLoudnessNormalization => {
            model.settings.rs_player_settings.loudness_normalization_enabled =
                !model.settings.rs_player_settings.loudness_normalization_enabled;
        }
        Msg::InputNormalizationTargetLufs(val) => {
            if let Ok(v) = val.parse::<f64>() {
                model.settings.rs_player_settings.loudness_normalization_target_lufs = v;
            }
        }
        Msg::DspAddFilter => {
            model.settings.rs_player_settings.dsp_settings.filters.push(FilterConfig {
                filter: DspFilter::Peaking {
                    freq: 1000.0,
                    gain: 0.0,
                    q: 0.707,
                },
                channels: vec![],
            });
        }
        Msg::DspRemoveAllFilters => {
            model.settings.rs_player_settings.dsp_settings.filters.clear();
        }
        Msg::DspRemoveFilter(index) => {
            if index < model.settings.rs_player_settings.dsp_settings.filters.len() {
                model.settings.rs_player_settings.dsp_settings.filters.remove(index);
            }
        }
        Msg::DspUpdateFilterType(index, filter_type) => {
            if let Some(filter_config) = model.settings.rs_player_settings.dsp_settings.filters.get_mut(index) {
                filter_config.filter = match filter_type {
                    FilterType::Peaking => DspFilter::Peaking { freq: 1000.0, gain: 0.0, q: 0.707 },
                    FilterType::LowShelf => DspFilter::LowShelf { freq: 80.0, gain: 0.0, q: Some(0.707), slope: None },
                    FilterType::HighShelf => DspFilter::HighShelf { freq: 12000.0, gain: 0.0, q: Some(0.707), slope: None },
                    FilterType::LowPass => DspFilter::LowPass { freq: 20000.0, q: 0.707 },
                    FilterType::HighPass => DspFilter::HighPass { freq: 20.0, q: 0.707 },
                    FilterType::BandPass => DspFilter::BandPass { freq: 1000.0, q: 0.707 },
                    FilterType::Notch => DspFilter::Notch { freq: 1000.0, q: 0.707 },
                    FilterType::AllPass => DspFilter::AllPass { freq: 1000.0, q: 0.707 },
                    FilterType::LowPassFO => DspFilter::LowPassFO { freq: 20000.0 },
                    FilterType::HighPassFO => DspFilter::HighPassFO { freq: 20.0 },
                    FilterType::LowShelfFO => DspFilter::LowShelfFO { freq: 80.0, gain: 0.0 },
                    FilterType::HighShelfFO => DspFilter::HighShelfFO { freq: 12000.0, gain: 0.0 },
                    FilterType::Gain => DspFilter::Gain { gain: 0.0 },
                };
            }
        }
        Msg::DspUpdateFilterValue(index, field, value) => {
             if let Some(filter_config) = model.settings.rs_player_settings.dsp_settings.filters.get_mut(index) {
                if let Ok(val) = value.parse::<f64>() {
                    match (&mut filter_config.filter, field) {
                        (DspFilter::Peaking { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::Peaking { gain, .. }, DspField::Gain) => *gain = val,
                        (DspFilter::Peaking { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::LowShelf { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::LowShelf { gain, .. }, DspField::Gain) => *gain = val,
                        (DspFilter::LowShelf { q, .. }, DspField::Q) => *q = Some(val),
                        (DspFilter::LowShelf { slope, .. }, DspField::Slope) => *slope = Some(val),

                        (DspFilter::HighShelf { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::HighShelf { gain, .. }, DspField::Gain) => *gain = val,
                        (DspFilter::HighShelf { q, .. }, DspField::Q) => *q = Some(val),
                        (DspFilter::HighShelf { slope, .. }, DspField::Slope) => *slope = Some(val),

                        (DspFilter::LowPass { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::LowPass { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::HighPass { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::HighPass { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::BandPass { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::BandPass { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::Notch { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::Notch { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::AllPass { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::AllPass { q, .. }, DspField::Q) => *q = val,

                        (DspFilter::LowPassFO { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::HighPassFO { freq, .. }, DspField::Freq) => *freq = val,

                        (DspFilter::LowShelfFO { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::LowShelfFO { gain, .. }, DspField::Gain) => *gain = val,

                        (DspFilter::HighShelfFO { freq, .. }, DspField::Freq) => *freq = val,
                        (DspFilter::HighShelfFO { gain, .. }, DspField::Gain) => *gain = val,

                        (DspFilter::Gain { gain }, DspField::Gain) => *gain = val,
                        _ => {}
                    }
                }
             }
        }
        Msg::DspUpdateFilterChannels(index, val) => {
            if let Some(filter_config) = model.settings.rs_player_settings.dsp_settings.filters.get_mut(index) {
                match val.as_str() {
                    "Left" => filter_config.channels = vec![0],
                    "Right" => filter_config.channels = vec![1],
                    _ => filter_config.channels = vec![],
                }
            }
        }
        Msg::DspApplySettings => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::UpdateDsp(model.settings.rs_player_settings.dsp_settings.clone())));
        }
        Msg::DspLoadPreset(index) => {
            if let Some(preset) = get_dsp_presets().get(index) {
                model.settings.rs_player_settings.dsp_settings.filters = preset.filters.clone();
            }
        }
        Msg::DspImportConfig(file_list) => {
            if let Some(file) = file_list.get(0) {
                let file = File::from(file);
                orders.perform_cmd(async move {
                    if let Ok(content) = read_as_text(&file).await {
                        Msg::DspConfigLoaded(content)
                    } else {
                        Msg::DspConfigLoaded(String::new()) // Error handling simplified
                    }
                });
            }
        }
        Msg::DspConfigLoaded(content) => {
            if !content.is_empty() {
                let filters = crate::dsp::parse_dsp_config(&content);
                if !filters.is_empty() {
                    model.settings.rs_player_settings.dsp_settings.filters = filters;
                }
            }
        }
        // --- Network Storage ---
        Msg::InputMountName(val) => { model.mount_form_name = val; }
        Msg::InputMountType(val) => {
            model.mount_form_type = if val == "Nfs" { NetworkMountType::Nfs } else { NetworkMountType::Smb };
        }
        Msg::InputMountServer(val) => { model.mount_form_server = val; }
        Msg::InputMountShare(val) => { model.mount_form_share = val; }
        Msg::InputMountUsername(val) => { model.mount_form_username = val; }
        Msg::InputMountPassword(val) => { model.mount_form_password = val; }
        Msg::InputMountDomain(val) => { model.mount_form_domain = val; }
        Msg::MountAdd => {
            if model.mount_form_server.is_empty() || model.mount_form_share.is_empty() {
                return;
            }
            // Auto-derive name from share if not provided
            let name = if model.mount_form_name.is_empty() {
                model.mount_form_share.replace(['/', ' '], "_")
            } else {
                model.mount_form_name.clone()
            };
            let config = NetworkMountConfig {
                name,
                mount_type: model.mount_form_type.clone(),
                server: model.mount_form_server.clone(),
                share: model.mount_form_share.clone(),
                username: if model.mount_form_username.is_empty() { None } else { Some(model.mount_form_username.clone()) },
                password: if model.mount_form_password.is_empty() { None } else { Some(model.mount_form_password.clone()) },
                domain: if model.mount_form_domain.is_empty() { None } else { Some(model.mount_form_domain.clone()) },
                mount_point: None,
            };
            let mount_point = format!("/mnt/rsplayer/{}", config.name);
            model.settings.network_storage_settings.mounts.push(config.clone());
            if !model.settings.metadata_settings.music_directories.contains(&mount_point) {
                model.settings.metadata_settings.music_directories.push(mount_point);
            }
            orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(StorageCommand::Mount(config))));
            // Clear form
            model.mount_form_name.clear();
            model.mount_form_server.clear();
            model.mount_form_share.clear();
            model.mount_form_username.clear();
            model.mount_form_password.clear();
            model.mount_form_domain.clear();
        }
        Msg::UnmountShare(name) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(StorageCommand::Unmount(name))));
        }
        Msg::MountRemove(name) => {
            let mount_point = model
                .settings
                .network_storage_settings
                .mounts
                .iter()
                .find(|m| m.name == name)
                .and_then(|m| m.mount_point.clone())
                .unwrap_or_else(|| format!("/mnt/rsplayer/{name}"));
            model.settings.network_storage_settings.mounts.retain(|m| m.name != name);
            model.mount_statuses.retain(|s| s.name != name);
            model.settings.metadata_settings.music_directories.retain(|d| d != &mount_point);
            orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(StorageCommand::Remove(name))));
        }
        Msg::MountShare(name) => {
            if let Some(mount_config) = model.settings.network_storage_settings.mounts.iter().find(|m| m.name == name) {
                orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(StorageCommand::Mount(mount_config.clone()))));
            }
        }
        Msg::MountStatusReceived(statuses) => {
            model.mount_statuses = statuses;
        }
        Msg::MusicDirStatusReceived(statuses) => {
            model.music_dir_statuses = statuses;
        }
        Msg::ExternalMountsReceived(mounts) => {
            model.external_mounts = mounts;
        }
        Msg::SaveExternalMount(mount_point) => {
            orders.send_msg(Msg::SendUserCommand(UserCommand::Storage(
                StorageCommand::SaveExternalMount(mount_point),
            )));
            // Re-fetch settings so the model reflects the newly saved mount
            orders.perform_cmd(async {
                let response = Request::get(API_SETTINGS_PATH)
                    .send()
                    .await
                    .expect("Failed to get settings from backend");

                let sett = response
                    .json::<Settings>()
                    .await
                    .expect("failed to deserialize to Configuration");
                Msg::SettingsFetched(sett)
            });
        }
        Msg::ToggleNetworkMounts => {
            model.network_mounts_open = !model.network_mounts_open;
        }
        Msg::SelectTheme(_) => {
            // Handled by the parent (lib.rs) — nothing to do here.
        }
        _ => {}
    }
}

// ------ ------
//     View
// ------ ------
#[allow(clippy::too_many_lines)]
pub fn view(model: &Model, current_theme: &str) -> Node<Msg> {
    let settings = &model.settings;
    div![
        style! {
            St::Background => "var(--overlay-bg)",
            St::BorderRadius => "8px",
        },
        view_spinner_modal(model.waiting_response),
        view_confirm_modal(&model.confirm_action),
        // Appearance
        section![
            style!{St::Padding => "0.5rem 0"},
            details![
                C!["settings-details"],
                summary![C!["settings-details__summary"], "Appearance"],
                div![C!["settings-details__body"], view_theme_picker(current_theme)],
            ],
        ],
        // Playback
        section![
            style!{St::Padding => "0.5rem 0"},
            details![
                C!["settings-details"],
                summary![C!["settings-details__summary"], "Playback"],
                div![
                    C!["settings-details__body"],
                    // Audio interface
                    div![
                        C!["field", "is-grouped", "is-grouped-multiline"],
                        div![C!["control"],
                            label!["Audio interface", C!["label","has-text-white"]],
                            div![
                                C!["select"],
                                select![
                                    option!["-- Select audio interface --"],
                                    option![
                                        IF!(model.settings.local_browser_playback => attrs!(At::Selected => "")),
                                        attrs! {At::Value => "browser"},
                                        "Local Browser Playback"
                                    ],
                                    model
                                    .settings
                                    .alsa_settings
                                    .available_audio_cards
                                    .iter()
                                    .map(|card| option![
                                        IF!(model.settings.alsa_settings.output_device.card_id == card.id && !model.settings.local_browser_playback => attrs!(At::Selected => "")),
                                        attrs! {At::Value => card.id},
                                        card.name.clone()
                                    ])],
                                input_ev(Ev::Change, |v| {
                                    Msg::InputAlsaCardChange(v)
                                }),
                            ],
                        ],
                        IF!(!model.settings.local_browser_playback => div![C!["control"],
                            label!["PCM Device", C!["label","has-text-white"]],
                            div![
                                C!["select"],
                                select![
                                    option!["-- Select pcm device --"],
                                    model.settings.alsa_settings.find_pcms_by_card_id(&model.selected_audio_card_id)
                                    .iter()
                                    .map(|pcmd|
                                        option![
                                            IF!(model.settings.alsa_settings.output_device.name == pcmd.name => attrs!(At::Selected => "")),
                                            attrs! {At::Value => pcmd.name},
                                            pcmd.description.clone()
                                        ]
                                    )
                                ],
                                input_ev(Ev::Change, Msg::InputAlsaPcmChange),
                            ],
                        ]),
                    ],
                    // Alsa mixer + Volume step
                    IF!(!settings.usb_settings.enabled =>
                        div![
                            C!["field", "is-grouped", "is-grouped-multiline", "mt-5"],
                            IF!(model.settings.volume_ctrl_settings.ctrl_device == VolumeCrtlType::Alsa =>
                                div![C!["control"],
                                    label!["Alsa mixer", C!["label","has-text-white"]],
                                    div![
                                        C!["select"],
                                        select![
                                            option!["-- Select mixer --"],
                                            model.settings.alsa_settings.find_mixers_by_card_id(&model.selected_audio_card_id)
                                            .iter()
                                            .map(|pcmd|
                                                option![
                                                    IF!(model.settings.volume_ctrl_settings.alsa_mixer_name.as_ref().is_some_and(|name| &pcmd.name == name) => attrs!(At::Selected => "")),
                                                    attrs! {At::Value => pcmd.name.clone()},
                                                    pcmd.name.clone()
                                                ]
                                            ),
                                            input_ev(Ev::Change, Msg::InputVolumeAlsaMixerChanged),
                                        ],
                                    ],
                                ]
                            ),
                            div![C!["control"],
                                label!["Volume step", C!["label","has-text-white"]],
                                input![
                                    C!["input"],
                                    attrs! {
                                        At::Value => model.settings.volume_ctrl_settings.volume_step
                                        At::Type => "number"
                                    },
                                    input_ev(Ev::Input, move |value| { Msg::InputVolumeStepChanged(value) }),
                                ],
                            ],
                        ]
                    ),
                    // Auto resume
                    div![
                        C!["field", "mt-5"],
                        ev(Ev::Click, |_| Msg::ToggleResumePlayback),
                        input![
                            C!["switch"],
                            attrs! {
                                At::Name => "resume_playback_cb"
                                At::Type => "checkbox"
                                At::Checked => settings.auto_resume_playback.as_at_value(),
                            },
                        ],
                        label![
                            C!["label","has-text-white"],
                            "Auto resume playback on start",
                            attrs! {
                                At::For => "resume_playback_cb"
                            }
                        ]
                    ],
                    view_rsp(&settings.rs_player_settings, settings.local_browser_playback),
                    view_metadata_storage(&model.settings.metadata_settings),
                ],
            ],
        ],

        // Audio Processing
        section![
            style!{St::Padding => "0.5rem 0"},
            details![
                C!["settings-details"],
                summary![C!["settings-details__summary"], "Audio Processing"],
                div![
                    C!["settings-details__body"],
                    div![
                        C!["field", "mt-5"],
                        ev(Ev::Click, |_| Msg::ToggleVuMeterEnabled),
                        input![
                            C!["switch"],
                            attrs! {
                                At::Name => "vu_meter_enabled_cb"
                                At::Type => "checkbox"
                                At::Checked => settings.rs_player_settings.vu_meter_enabled.as_at_value(),
                            },
                        ],
                        label![
                            C!["label","has-text-white"],
                            "Enable VU meter",
                            attrs! {
                                At::For => "vu_meter_enabled_cb"
                            }
                        ]
                    ],
                    div![
                        C!["field", "mt-5"],
                        ev(Ev::Click, |_| Msg::ToggleLoudnessNormalization),
                        input![
                            C!["switch"],
                            attrs! {
                                At::Name => "loudness_norm_cb"
                                At::Type => "checkbox"
                                At::Checked => settings.rs_player_settings.loudness_normalization_enabled.as_at_value(),
                            },
                        ],
                        label![
                            C!["label","has-text-white"],
                            "Enable loudness normalization (EBU R128)",
                            attrs! {
                                At::For => "loudness_norm_cb"
                            }
                        ]
                    ],
                    IF!(settings.rs_player_settings.loudness_normalization_enabled =>
                        div![
                            div![
                                C!["field", "mt-2"],
                                label![C!["label", "has-text-white"], "Target loudness (LUFS)"],
                                div![
                                    C!["control"],
                                    input![
                                        C!["input", "is-small"],
                                        attrs! {
                                            At::Type => "number"
                                            At::Value => settings.rs_player_settings.loudness_normalization_target_lufs
                                            At::Step => "0.5"
                                            At::Min => "-30"
                                            At::Max => "-5"
                                        },
                                        input_ev(Ev::Change, Msg::InputNormalizationTargetLufs),
                                    ]
                                ]
                            ],
                            div![
                                C!["notification", "mt-4"],
                                i![C!["material-icons", "mr-2", "is-size-6"], "info"],
                                span![
                                    "Loudness analysis runs automatically in the background while playback is stopped. ",
                                    "Each song is measured once (EBU R128) and the result is stored permanently. ",
                                    "You can track progress on the ",
                                    a![attrs! { At::Href => "/#/library/stats" }, "Library Statistics"],
                                    " page."
                                ],
                            ]
                        ]
                    ),
                    view_dsp_settings(&settings.rs_player_settings),
                ],
            ],
        ],

        // Music Library
        section![
            style!{St::Padding => "0.5rem 0"},
            details![
                C!["settings-details"],
                attrs! { At::Open => true },
                summary![C!["settings-details__summary"], "Music Library"],
                div![
                    C!["settings-details__body"],
                    view_music_directories(model),
                    view_network_storage(model),
                    div![
                        C!["field", "is-grouped", "is-grouped-right", "mt-4"],
                        div![
                            C!["control"],
                            button![
                                C!["button"],
                                ev(Ev::Click, move |_| Msg::ClickRescanMetadataButton(false)),
                                "Update library"
                            ],
                        ],
                        div![
                            C!["control"],
                            button![
                                C!["button", "is-warning"],
                                ev(Ev::Click, move |_| Msg::ShowConfirm(ConfirmAction::FullRescan)),
                                "Full rescan"
                            ],
                        ],
                    ],
                ],
            ],
        ],

        // Hardware
        section![
            style!{St::Padding => "0.5rem 0"},
            details![
                C!["settings-details"],
                summary![C!["settings-details__summary"], "Hardware"],
                div![
                    C!["settings-details__body"],
                    div![
                        C!["field"],
                        ev(Ev::Click, |_| Msg::ToggleUsbEnabled),
                        input![
                            C!["switch"],
                            attrs! {
                                At::Name => "usb_cb"
                                At::Type => "checkbox"
                                At::Checked => settings.usb_settings.enabled.as_at_value(),
                            },
                        ],
                        label![
                            C!["label","has-text-white"],
                            "Enable link with rsplayer firmware",
                            attrs! {
                                At::For => "usb_cb"
                            }
                        ]
                    ],
                    div![
                        C!["buttons", "mt-4"],
                        IF!(model.settings.usb_settings.enabled =>
                            button![
                                C!["button", "is-danger", "is-small"],
                                "Power Off",
                                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::SetFirmwarePower(false)))
                            ]
                        ),
                        IF!(model.settings.usb_settings.enabled =>
                            button![
                                C!["button", "is-success", "is-small"],
                                "Power On",
                                ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::SetFirmwarePower(true)))
                            ]
                        )
                    ],
                ],
            ],
        ],

        // buttons
        div![
            C!["buttons"],
                button![
                    IF!(model.settings.validate().is_err() => attrs!{ At::Disabled => ""}),
                    C!["button"],
                    "Save & restart player",
                    ev(Ev::Click, |_| Msg::ShowConfirm(ConfirmAction::SaveAndRestartPlayer))
                ],
                button![
                    C!["button", "is-warning"],
                    "Restart player",
                    ev(Ev::Click, |_| Msg::ShowConfirm(ConfirmAction::RestartPlayer))
                ],
                button![
                    C!["button", "is-danger"],
                    "Restart system",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(
                        SystemCommand::RestartSystem
                    ))
                ],
                button![
                    C!["button", "is-danger"],
                    "Shutdown system",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::PowerOff))
                ]
        ],

        // version
        div![
            style! {
                St::TextAlign => "center",
                St::Padding => "1rem",
                St::Opacity => "0.6",
            },
            p![C!["has-text-grey-light", "is-size-7"],
                &format!("RSPlayer v{}", settings.version)
            ]
        ]
    ]
}
fn view_confirm_modal(confirm_action: &Option<ConfirmAction>) -> Node<Msg> {
    let Some(action) = confirm_action else {
        return empty!();
    };
    let message = match action {
        ConfirmAction::FullRescan => "Full rescan will destroy the current music database and rebuild it from scratch. All metadata and playback state will be lost. Are you sure?",
        ConfirmAction::RestartPlayer => "If the player was not started via systemd, it will exit and must be started again manually. Are you sure you want to restart?",
        ConfirmAction::SaveAndRestartPlayer => "Settings will be saved and the player will restart. If the player was not started via systemd, it will exit and must be started again manually. Are you sure?",
        ConfirmAction::RemoveMusicDirectory(_) => "This will remove the directory from music sources. Songs from this directory will no longer be scanned. No data or directory will be deleted. Are you sure?",
        ConfirmAction::RemoveNetworkMount(_) => "This will unmount and remove the network share. Songs from this mount will no longer be accessible. No data or directory will be deleted. Are you sure?",
    };
    div![
        C!["modal", "is-active"],
        div![
            C!["modal-background"],
            ev(Ev::Click, |_| Msg::ConfirmCancelled)
        ],
        div![
            C!["modal-card"],
            header![
                C!["modal-card-head"],
                p![C!["modal-card-title"], "Warning"],
                button![
                    C!["delete"],
                    attrs! { At::AriaLabel => "close" },
                    ev(Ev::Click, |_| Msg::ConfirmCancelled)
                ]
            ],
            section![
                C!["modal-card-body"],
                p![message]
            ],
            footer![
                C!["modal-card-foot"],
                button![
                    C!["button", "is-warning"],
                    "Confirm",
                    ev(Ev::Click, |_| Msg::ConfirmAccepted)
                ],
                button![
                    C!["button"],
                    "Cancel",
                    ev(Ev::Click, |_| Msg::ConfirmCancelled)
                ]
            ]
        ]
    ]
}

fn view_validation_icon<Ms>(val: &impl Validate, key: &str) -> Node<Ms> {
    let class = if let Err(errors) = val.validate() {
        if errors.errors().contains_key(key) {
            "fa-exclamation-triangle"
        } else {
            "fa-check"
        }
    } else {
        "fa-check"
    };

    span![C!["icon", "is-small", "is-right"], i![C!["fas", class]]]
}

// ------ sub view functions ------

fn view_metadata_storage(_metadata_settings: &MetadataStoreSettings) -> Node<Msg> {
    empty!()
}

fn view_rsp(rsp_settings: &RsPlayerSettings, local_browser_playback: bool) -> Node<Msg> {
    if local_browser_playback {
        return empty!();
    }
    div![
        C!["field"],
        details![
            C!["settings-details", "mt-5"],
            summary![C!["settings-details__summary"], "Advanced"],
            div![
                C!["settings-details__body"],
                label!["Input buffer size (MB) (1-200)", C!["label", "has-text-white", "mt-5"]],
                div![
                    C!["control", "has-icons-right"],
                    style! {St::Width => "max-content"},
                    input![
                        C!["input"],
                        attrs! {At::Value => rsp_settings.input_stream_buffer_size_mb, At::Type => "number"},
                        input_ev(Ev::Input, move |value| { Msg::InputRspInputBufferSizeChange(value) }),
                    ],
                    view_validation_icon(rsp_settings, "input_stream_buffer_size_mb")
                ],
                label!["Ring buffer size (1-10000ms)", C!["label", "has-text-white", "mt-5"]],
                p![
                    C!["help", "has-text-grey-light", "mb-2"],
                    "Software ring buffer between the decoder and ALSA. Default: 1000 ms. \
                     Increase if you hear dropouts with CPU-intensive formats (e.g. APE at high compression)."
                ],
                div![
                    C!["control", "has-icons-right"],
                    style! {St::Width => "max-content"},
                    input![
                        C!["input"],
                        attrs! {At::Value => rsp_settings.ring_buffer_size_ms, At::Type => "number"},
                        input_ev(Ev::Input, move |value| { Msg::InputRspAudioBufferSizeChange(value) }),
                    ],
                    view_validation_icon(rsp_settings, "ring_buffer_size_ms")
                ],
                label!["Player thread priority (1-99)", C!["label", "has-text-white", "mt-5"]],
                div![
                    C!["control", "has-icons-right"],
                    style! {St::Width => "max-content"},
                    input![
                        C!["input"],
                        attrs! {At::Value => rsp_settings.player_threads_priority, At::Type => "number"},
                        input_ev(Ev::Input, move |value| { Msg::InputRspThreadPriorityChange(value) }),
                    ],
                    view_validation_icon(rsp_settings, "player_threads_priority")
                ],
                label!["Fixed output sample rate", C!["label", "has-text-white", "mt-5"]],
                p![
                    C!["help", "has-text-grey-light", "mb-2"],
                    "Force all tracks to be resampled to this rate. Use only if auto-detection fails for your device."
                ],
                div![
                    C!["select"],
                    select![
                        input_ev(Ev::Input, Msg::InputRspFixedSampleRateChange),
                        option![
                            attrs! { At::Value => "" },
                            IF!(rsp_settings.fixed_output_sample_rate.is_none() => attrs! { At::Selected => true }),
                            "Auto (recommended)"
                        ],
                        [44100u32, 48000, 88200, 96000, 176400, 192000].iter().map(|&rate| {
                            option![
                                attrs! { At::Value => rate },
                                IF!(rsp_settings.fixed_output_sample_rate == Some(rate) => attrs! { At::Selected => true }),
                                format!("{} Hz", rate),
                            ]
                        }),
                    ],
                ],
                div![
                    C!["field", "mt-5"],
                    ev(Ev::Click, |_| Msg::ToggleRspAlsaBufferSize),
                    input![
                        C!["switch"],
                        attrs! {
                            At::Name => "alsabufsize_cb"
                            At::Type => "checkbox"
                            At::Checked => rsp_settings.alsa_buffer_size.is_some().as_at_value(),
                        },
                    ],
                    label![
                        C!["label", "has-text-white"],
                        "Override ALSA period size (frames)",
                        attrs! { At::For => "alsabufsize_cb" }
                    ]
                ],
                p![
                    C!["help", "has-text-grey-light", "mb-2"],
                    "When not overridden, rsplayer uses 4096 frames by default. \
                     This is the ALSA hardware period — how many frames ALSA processes per interrupt. \
                     Larger values reduce USB scheduling pressure and fix ",
                    code!["alsa::poll() POLLERR"],
                    " on async USB audio devices (e.g. Amanero Combo768 on RPi). \
                     At 44.1 kHz: 4096 frames ≈ 93 ms. Leave unset unless you have a specific reason."
                ],
                IF!(rsp_settings.alsa_buffer_size.is_some() =>
                    div![
                        C!["field"],
                        div![
                            C!["control"],
                            input![
                                C!["input"],
                                attrs! {
                                    At::Value => rsp_settings.alsa_buffer_size.unwrap_or(4096),
                                    At::Type => "number"
                                },
                                input_ev(Ev::Input, move |value| { Msg::InputRspAlsaBufferSizeChange(value) }),
                            ],
                        ],
                    ]
                ),
            ],
        ],
    ]
}

fn view_dsp_settings(rsp_settings: &RsPlayerSettings) -> Node<Msg> {
    div![
        div![
            C!["field", "mt-5"],
            ev(Ev::Click, |_| Msg::ToggleDspEnabled),
            input![
                C!["switch"],
                attrs! {
                    At::Name => "dsp_enabled_cb"
                    At::Type => "checkbox"
                    At::Checked => rsp_settings.dsp_settings.enabled.as_at_value(),
                },
            ],
            label![
                C!["label","has-text-white"],
                "Enable DSP processing",
                attrs! {
                    At::For => "dsp_enabled_cb"
                }
            ]
        ],
        IF!(rsp_settings.dsp_settings.enabled =>
            div![
                C!["box", "has-background-dark", "mt-4"],
                div![
                    C!["field", "mb-4"],
                    label!["Load Preset", C!["label", "has-text-white"]],
                    div![
                        C!["control"],
                        div![
                            C!["select"],
                            select![
                                option![attrs!{At::Value => ""}, "Select a preset..."],
                                get_dsp_presets().iter().enumerate().map(|(i, preset)| {
                                    option![
                                        attrs! {At::Value => i},
                                        &preset.name
                                    ]
                                }),
                                input_ev(Ev::Change, |val| {
                                    if let Ok(idx) = val.parse::<usize>() {
                                        Msg::DspLoadPreset(idx)
                                    } else {
                                        Msg::DspLoadPreset(999) // Invalid index to do nothing
                                    }
                                })
                            ]
                        ]
                    ]
                ],
                rsp_settings.dsp_settings.filters.iter().enumerate().map(|(index, filter)| {
                    view_filter(index, filter)
                }),
                div![
                    C!["buttons", "mt-4"],
                    button![
                        C!["button", "is-primary", "is-small"],
                        "Add Filter",
                        ev(Ev::Click, |_| Msg::DspAddFilter)
                    ],
                    button![
                        C!["button", "is-danger", "is-small"],
                        "Remove All",
                        ev(Ev::Click, |_| Msg::DspRemoveAllFilters)
                    ],
                    button![
                        C!["button", "is-warning", "is-small"],
                        "Apply (Live)",
                        ev(Ev::Click, |_| Msg::DspApplySettings)
                    ],
                    div![
                        C!["file", "is-primary", "is-small", "ml-2"],
                        label![
                            C!["file-label"],
                            input![
                                C!["file-input"],
                                attrs! { At::Type => "file", At::Accept => ".yml,.yaml" },
                                ev(Ev::Change, |event| {
                                    let target = event.target().unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
                                    if let Some(files) = target.files() {
                                        Msg::DspImportConfig(files)
                                    } else {
                                        Msg::DspApplySettings // Dummy
                                    }
                                })
                            ],
                            span![
                                C!["file-cta"],
                                span![C!["file-icon"], i![C!["fas", "fa-upload"]]],
                                span![C!["file-label"], "Import CamillaDSP config"]
                            ]
                        ]
                    ]
                ]
            ]
        )
    ]
}

fn view_filter(index: usize, filter_config: &FilterConfig) -> Node<Msg> {
    let filter = &filter_config.filter;
    let (current_type, freq, gain, q, slope) = match filter {
        DspFilter::Peaking { freq, gain, q } => (FilterType::Peaking, Some(*freq), Some(*gain), Some(*q), None),
        DspFilter::LowShelf { freq, gain, q, slope } => (FilterType::LowShelf, Some(*freq), Some(*gain), *q, *slope),
        DspFilter::HighShelf { freq, gain, q, slope } => (FilterType::HighShelf, Some(*freq), Some(*gain), *q, *slope),
        DspFilter::LowPass { freq, q } => (FilterType::LowPass, Some(*freq), None, Some(*q), None),
        DspFilter::HighPass { freq, q } => (FilterType::HighPass, Some(*freq), None, Some(*q), None),
        DspFilter::BandPass { freq, q } => (FilterType::BandPass, Some(*freq), None, Some(*q), None),
        DspFilter::Notch { freq, q } => (FilterType::Notch, Some(*freq), None, Some(*q), None),
        DspFilter::AllPass { freq, q } => (FilterType::AllPass, Some(*freq), None, Some(*q), None),
        DspFilter::LowPassFO { freq } => (FilterType::LowPassFO, Some(*freq), None, None, None),
        DspFilter::HighPassFO { freq } => (FilterType::HighPassFO, Some(*freq), None, None, None),
        DspFilter::LowShelfFO { freq, gain } => (FilterType::LowShelfFO, Some(*freq), Some(*gain), None, None),
        DspFilter::HighShelfFO { freq, gain } => (FilterType::HighShelfFO, Some(*freq), Some(*gain), None, None),
        DspFilter::LinkwitzTransform { .. } => (FilterType::Gain, None, None, None, None), // Placeholder
        DspFilter::Gain { gain } => (FilterType::Gain, None, Some(*gain), None, None),
    };

    let current_channel = if filter_config.channels.is_empty() {
        "Global"
    } else if filter_config.channels == vec![0] {
        "Left"
    } else if filter_config.channels == vec![1] {
        "Right"
    } else {
        "Custom"
    };

    div![
        C!["field", "is-grouped", "is-grouped-multiline", "p-3", "mb-3"],
        style! { St::Border => "1px solid #4a4a4a", St::BorderRadius => "4px"},

        // Channel Selector
        div![
            C!["control"],
            label!["Channel", C!["label", "has-text-white"]],
            div![
                C!["select"],
                select![
                    option![attrs!{At::Value => "Global"}, IF!(current_channel == "Global" => attrs!{At::Selected => ""}), "Global"],
                    option![attrs!{At::Value => "Left"}, IF!(current_channel == "Left" => attrs!{At::Selected => ""}), "Left"],
                    option![attrs!{At::Value => "Right"}, IF!(current_channel == "Right" => attrs!{At::Selected => ""}), "Right"],
                    input_ev(Ev::Change, move |val| Msg::DspUpdateFilterChannels(index, val))
                ]
            ]
        ],

        // Filter Type
        div![
            C!["control"],
            label!["Type", C!["label", "has-text-white"]],
            div![
                C!["select"],
                select![
                    FilterType::iter().map(|ft| {
                        option![
                            attrs! { At::Value => ft.to_string() },
                            IF!(ft == current_type => attrs!{At::Selected => ""}),
                            ft.to_string()
                        ]
                    }),
                    input_ev(Ev::Change, move |val| {
                        let new_type = match val.as_str() {
                            "Peaking" => FilterType::Peaking,
                            "LowShelf" => FilterType::LowShelf,
                            "HighShelf" => FilterType::HighShelf,
                            "LowPass" => FilterType::LowPass,
                            "HighPass" => FilterType::HighPass,
                            "Gain" => FilterType::Gain,
                            _ => unreachable!(),
                        };
                        Msg::DspUpdateFilterType(index, new_type)
                    })
                ]
            ]
        ],

        // Freq
        freq.map(|f| div![
            C!["control"],
            label!["Freq", C!["label", "has-text-white"]],
            input![
                C!["input"],
                attrs! { At::Type => "number", At::Value => f.to_string() },
                input_ev(Ev::Input, move |v| Msg::DspUpdateFilterValue(index, DspField::Freq, v))
            ]
        ]),

        // Gain
        gain.map(|g| div![
            C!["control"],
            label!["Gain", C!["label", "has-text-white"]],
            input![
                C!["input"],
                attrs! { At::Type => "number", At::Value => g.to_string() },
                input_ev(Ev::Input, move |v| Msg::DspUpdateFilterValue(index, DspField::Gain, v))
            ]
        ]),
        
        // Q
        q.map(|q_val| div![
            C!["control"],
            label!["Q", C!["label", "has-text-white"]],
            input![
                C!["input"],
                attrs! { At::Type => "number", At::Value => q_val.to_string() },
                input_ev(Ev::Input, move |v| Msg::DspUpdateFilterValue(index, DspField::Q, v))
            ]
        ]),

        // Slope
        slope.map(|s_val| div![
            C!["control"],
            label!["Slope", C!["label", "has-text-white"]],
            input![
                C!["input"],
                attrs! { At::Type => "number", At::Value => s_val.to_string() },
                input_ev(Ev::Input, move |v| Msg::DspUpdateFilterValue(index, DspField::Slope, v))
            ]
        ]),

        // Remove button
        div![
            C!["control", "is-flex", "is-align-items-flex-end"],
            button![
                C!["button", "is-danger", "is-small"],
                "Remove",
                ev(Ev::Click, move |_| Msg::DspRemoveFilter(index))
            ]
        ]
    ]
}



/// (bg, primary-text, accent, ui-elements, label)
const THEMES: &[(&str, &str, &str, &str, &str)] = &[
    ("dark",          "#121212", "#FFFFFF", "#1DB954", "#282828", ),
    ("light",         "#f5f5f5", "#1a1a1a", "#1a8f3c", "#e0e0e0", ),
    ("solarized",     "#002b36", "#eee8d5", "#2aa198", "#073642", ),
    ("dracula",       "#282a36", "#f8f8f2", "#bd93f9", "#44475a", ),
    ("nord",          "#2e3440", "#eceff4", "#88c0d0", "#3b4252", ),
    ("rose-pine",     "#191724", "#e0def4", "#eb6f92", "#26233a", ),
    ("ocean",         "#0f1923", "#cdd6f4", "#4fc3f7", "#1a2a3a", ),
    ("gruvbox",       "#282828", "#ebdbb2", "#b8bb26", "#3c3836", ),
    ("catppuccin",    "#1e1e2e", "#cdd6f4", "#cba6f7", "#313244", ),
    ("high-contrast", "#000000", "#ffffff", "#00ff00", "#1a1a1a", ),
];

const THEME_LABELS: &[(&str, &str)] = &[
    ("dark",          "Dark"),
    ("light",         "Light"),
    ("solarized",     "Solarized"),
    ("dracula",       "Dracula"),
    ("nord",          "Nord"),
    ("rose-pine",     "Rose Pine"),
    ("ocean",         "Ocean"),
    ("gruvbox",       "Gruvbox"),
    ("catppuccin",    "Catppuccin"),
    ("high-contrast", "Hi-Contrast"),
];

fn view_theme_picker(current_theme: &str) -> Node<Msg> {
    div![
        C!["theme-picker"],
        THEMES.iter().zip(THEME_LABELS.iter()).map(|((id, bg, _text, accent, ui), (_id2, label))| {
            let theme_id = id.to_string();
            let is_active = *id == current_theme;
            div![
                C!["theme-card", IF!(is_active => "is-active")],
                // colour swatch: bg | ui | accent
                div![
                    C!["theme-card__swatch"],
                    span![style! { St::Background => *bg }],
                    span![style! { St::Background => *ui }],
                    span![style! { St::Background => *accent }],
                ],
                span![C!["theme-card__name"], *label],
                ev(Ev::Click, move |_| Msg::SelectTheme(theme_id)),
            ]
        })
    ]
}

fn view_music_directories(model: &Model) -> Node<Msg> {
    let mount_points: Vec<String> = model.settings.network_storage_settings.mounts
        .iter()
        .map(|m| {
            m.mount_point
                .clone()
                .unwrap_or_else(|| format!("/mnt/rsplayer/{}", m.name))
        })
        .collect();
    div![
        // Local directory entries (same style as network mounts)
        model.settings.metadata_settings.music_directories.iter().enumerate()
            .filter(|(_, dir)| !mount_points.contains(dir))
            .map(|(idx, dir)| {
                let dir_status = model.music_dir_statuses.iter().find(|s| s.path == *dir);
                let status_label = match dir_status {
                    Some(s) if s.readable && s.writable => "Read / Write",
                    Some(s) if s.readable => "Read only",
                    Some(_) => "Not accessible",
                    None => "Unknown",
                };
                let status_class = match dir_status {
                    Some(s) if s.readable => "is-success",
                    Some(_) => "is-danger",
                    None => "is-warning",
                };
                div![
                    C!["field", "is-grouped", "is-grouped-multiline", "p-3", "mb-3"],
                    style! { St::Border => "1px solid #4a4a4a", St::BorderRadius => "4px" },
                    div![
                        C!["control"],
                        span![
                            C!["tag", status_class, "mr-2"],
                            status_label
                        ],
                    ],
                    div![
                        C!["control", "is-expanded"],
                        label![C!["label", "has-text-white"], format!("{dir}")],
                        p![C!["has-text-grey-light"], "Local directory"],
                    ],
                    div![
                        C!["control"],
                        button![
                            C!["button", "is-danger", "is-small"],
                            "Remove",
                            ev(Ev::Click, move |_| Msg::ShowConfirm(ConfirmAction::RemoveMusicDirectory(idx)))
                        ],
                    ],
                ]
            }),
    ]
}

fn view_network_storage(model: &Model) -> Node<Msg> {
    div![
        // Existing mounts
        model.settings.network_storage_settings.mounts.iter().map(|mount| {
            let mount_name2 = mount.name.clone();
            let status = model.mount_statuses.iter().find(|s| s.name == mount.name);
            let is_mounted = status.is_some_and(|s| s.is_mounted);
            let mount_point = mount
                .mount_point
                .clone()
                .unwrap_or_else(|| format!("/mnt/rsplayer/{}", mount.name));
            let type_label = match mount.mount_type { NetworkMountType::Smb => "SMB", NetworkMountType::Nfs => "NFS" };
            let source = match mount.mount_type {
                NetworkMountType::Smb => format!("//{}/{}", mount.server, mount.share),
                NetworkMountType::Nfs => format!("{}:{}", mount.server, mount.share),
            };
            let (status_label, status_class) = match status {
                Some(s) if s.readable && s.writable => ("Read / Write", "is-success"),
                Some(s) if s.readable => ("Read only", "is-warning"),
                Some(s) if s.is_mounted => ("Not accessible", "is-danger"),
                Some(_) => ("Not mounted", "is-danger"),
                None => ("Unknown", "is-light"),
            };
            div![
                C!["field", "is-grouped", "is-grouped-multiline", "p-3", "mb-3"],
                style! { St::Border => "1px solid #4a4a4a", St::BorderRadius => "4px" },
                div![
                    C!["control"],
                    span![
                        C!["tag", status_class, "mr-2"],
                        status_label
                    ],
                ],
                div![
                    C!["control", "is-expanded"],
                    label![C!["label", "has-text-white"], format!("{} ({})", mount.name, type_label)],
                    p![C!["has-text-grey-light"], format!("{source} → {mount_point}")],
                ],
                div![
                    C!["control"],
                    div![
                        C!["buttons"],
                        {
                            let mount_name = mount.name.clone();
                            if is_mounted {
                                button![
                                    C!["button", "is-warning", "is-small"],
                                    "Unmount",
                                    ev(Ev::Click, move |_| Msg::UnmountShare(mount_name))
                                ]
                            } else {
                                button![
                                    C!["button", "is-success", "is-small"],
                                    "Mount",
                                    ev(Ev::Click, move |_| Msg::MountShare(mount_name))
                                ]
                            }
                        },
                        button![
                            C!["button", "is-danger", "is-small"],
                            "Remove",
                            ev(Ev::Click, move |_| Msg::ShowConfirm(ConfirmAction::RemoveNetworkMount(mount_name2)))
                        ],
                    ]
                ],
            ]
        }),

        // Add local directory form
        div![
            C!["box", "has-background-dark", "mt-4"],
            h1![C!["title", "is-6", "has-text-white"], "Add Local Directory"],
            div![
                C!["field", "has-addons"],
                div![
                    C!["control", "is-expanded"],
                    input![
                        C!["input", "is-small"],
                        attrs! { At::Type => "text", At::Value => &model.new_music_dir, At::Placeholder => "/path/to/music" },
                        input_ev(Ev::Input, Msg::InputNewMusicDir),
                    ],
                ],
                div![
                    C!["control"],
                    button![
                        C!["button", "is-small", "is-primary"],
                        ev(Ev::Click, |_| Msg::AddMusicDirectory),
                        "Add"
                    ],
                ],
            ],
        ],

        // Network mount management (collapsible)
        div![
            C!["settings-details", "mt-4"],
            div![
                C!["settings-details__summary"],
                "Network Mounts",
                i![
                    C!["material-icons"],
                    style! { St::MarginLeft => "auto", St::Transition => "transform 0.2s ease",
                             St::Transform => if model.network_mounts_open { "rotate(180deg)" } else { "rotate(0deg)" } },
                    "expand_more"
                ],
                ev(Ev::Click, |_| Msg::ToggleNetworkMounts),
            ],
            IF!(model.network_mounts_open =>
            div![
                C!["settings-details__body"],
                div![
                    C!["box", "has-background-dark", "mt-3"],
            h1![C!["title", "is-6", "has-text-white"], "Add Network Mount"],
            div![
                C!["field", "is-grouped", "is-grouped-multiline"],
                div![
                    C!["control"],
                    label![C!["label", "has-text-white"], "Name (optional)"],
                    input![
                        C!["input", "is-small"],
                        attrs! { At::Type => "text", At::Value => &model.mount_form_name, At::Placeholder => "auto from share" },
                        input_ev(Ev::Input, Msg::InputMountName),
                    ],
                ],
                div![
                    C!["control"],
                    label![C!["label", "has-text-white"], "Type"],
                    div![
                        C!["select", "is-small"],
                        select![
                            option![
                                attrs! { At::Value => "Smb" },
                                IF!(matches!(model.mount_form_type, NetworkMountType::Smb) => attrs!(At::Selected => "")),
                                "SMB/CIFS"
                            ],
                            option![
                                attrs! { At::Value => "Nfs" },
                                IF!(matches!(model.mount_form_type, NetworkMountType::Nfs) => attrs!(At::Selected => "")),
                                "NFS"
                            ],
                            input_ev(Ev::Change, Msg::InputMountType),
                        ],
                    ],
                ],
                div![
                    C!["control"],
                    label![C!["label", "has-text-white"], "Server *"],
                    input![
                        C!["input", "is-small"],
                        attrs! { At::Type => "text", At::Value => &model.mount_form_server, At::Placeholder => "192.168.1.100", At::Required => true },
                        input_ev(Ev::Input, Msg::InputMountServer),
                    ],
                ],
                div![
                    C!["control"],
                    label![C!["label", "has-text-white"], "Share *"],
                    input![
                        C!["input", "is-small"],
                        attrs! { At::Type => "text", At::Value => &model.mount_form_share, At::Placeholder => "music", At::Required => true },
                        input_ev(Ev::Input, Msg::InputMountShare),
                    ],
                ],
            ],
            IF!(matches!(model.mount_form_type, NetworkMountType::Smb) =>
                div![
                    C!["field", "is-grouped", "is-grouped-multiline", "mt-3"],
                    div![
                        C!["control"],
                        label![C!["label", "has-text-white"], "Username (optional)"],
                        input![
                            C!["input", "is-small"],
                            attrs! { At::Type => "text", At::Value => &model.mount_form_username, At::Placeholder => "guest" },
                            input_ev(Ev::Input, Msg::InputMountUsername),
                        ],
                    ],
                    div![
                        C!["control"],
                        label![C!["label", "has-text-white"], "Password (optional)"],
                        input![
                            C!["input", "is-small"],
                            attrs! { At::Type => "password", At::Value => &model.mount_form_password },
                            input_ev(Ev::Input, Msg::InputMountPassword),
                        ],
                    ],
                    div![
                        C!["control"],
                        label![C!["label", "has-text-white"], "Domain (optional)"],
                        input![
                            C!["input", "is-small"],
                            attrs! { At::Type => "text", At::Value => &model.mount_form_domain, At::Placeholder => "WORKGROUP" },
                            input_ev(Ev::Input, Msg::InputMountDomain),
                        ],
                    ],
                ]
            ),
            div![
                C!["field", "mt-3"],
                button![
                    C!["button", "is-primary", "is-small"],
                    "Mount",
                    ev(Ev::Click, |_| Msg::MountAdd),
                ],
            ],
            div![
                C!["notification", "mt-3", "p-3"],
                i![C!["material-icons", "mr-2", "is-size-6"], "info"],
                span![
                    "Mounts are created under /mnt/rsplayer/<name> and automatically added as music directories.",
                ],
            ],
        ],
        view_external_mounts(model),
        ] // settings-details__body
        ), // IF
        ] // div settings-details
    ]
}

#[allow(clippy::future_not_send)]
fn view_external_mounts(model: &Model) -> Node<Msg> {
    if model.external_mounts.is_empty() {
        return empty![];
    }
    div![
        C!["box", "mt-4"],
        style! {
            St::Background => "rgba(32, 64, 100, 0.4)",
            St::Border => "1px solid #3273dc",
            St::BorderRadius => "6px",
        },
        h1![
            C!["title", "is-6", "has-text-white"],
            "Detected External Network Mounts"
        ],
        p![
            C!["has-text-grey-light", "mb-3", "is-size-7"],
            "These network mounts were found on the system but are not managed by rsplayer. ",
            "Click Save to add them as music sources."
        ],
        model.external_mounts.iter().map(|ext| {
            let mp = ext.mount_point.clone();
            let (status_label, status_class) = match (ext.readable, ext.writable) {
                (true, true) => ("Read / Write", "is-success"),
                (true, false) => ("Read only", "is-warning"),
                _ => ("Not accessible", "is-danger"),
            };
            div![
                C!["field", "is-grouped", "is-grouped-multiline", "p-3", "mb-3"],
                style! { St::Border => "1px solid #4a6a8a", St::BorderRadius => "4px" },
                div![
                    C!["control"],
                    span![C!["tag", status_class, "mr-2"], status_label],
                ],
                div![
                    C!["control", "is-expanded"],
                    label![
                        C!["label", "has-text-white"],
                        format!("{} ({})", ext.source, ext.fs_type.to_uppercase())
                    ],
                    p![C!["has-text-grey-light"], &ext.mount_point],
                ],
                div![
                    C!["control"],
                    button![
                        C!["button", "is-info", "is-small"],
                        "Save",
                        ev(Ev::Click, move |_| Msg::SaveExternalMount(mp))
                    ],
                ],
            ]
        }),
    ]
}

async fn save_settings(settings: Settings, query: String) -> Result<String, Error> {
    let response = Request::post(format!("{API_SETTINGS_PATH}?{query}").as_str())
        .json(&settings)?
        .send()
        .await?;
    response.text().await
}
