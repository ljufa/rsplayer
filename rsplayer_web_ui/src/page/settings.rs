
use api_models::{
    common::{MetadataCommand::RescanMetadata, SystemCommand, UserCommand, VolumeCrtlType},
    settings::{DspFilter, FilterConfig, MetadataStoreSettings, RsPlayerSettings, Settings},
    validator::Validate,
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use gloo_console::{error, log};
use gloo_file::futures::read_as_text;
use gloo_file::File;
use gloo_net::{http::Request, Error};
use seed::{attrs, button, div, h1, i, input, label, option, prelude::*, section, select, span, style, C, IF};
use wasm_bindgen::JsCast;
use web_sys::FileList;

use crate::view_spinner_modal;

const API_SETTINGS_PATH: &str = "/api/settings";

// ------ ------
//     Model

#[derive(Debug)]
pub struct Model {
    settings: Settings,
    selected_audio_card_id: String,
    waiting_response: bool,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    // ---- on off toggles ----
    ToggleUsbEnabled,
    ToggleResumePlayback,
    ToggleRspAlsaBufferSize,
    // ---- Input capture ----
    InputMetadataMusicDirectoryChanged(String),
    InputAlsaCardChange(String),
    InputAlsaPcmChange(String),
    InputVolumeStepChanged(String),

    InputRspInputBufferSizeChange(String),
    InputRspAudioBufferSizeChange(String),
    InputRspAlsaBufferSizeChange(String),
    InputRspThreadPriorityChange(String),
    InputVolumeAlsaMixerChanged(String),
    ClickRescanMetadataButton(bool),

    InputAlsaDeviceChanged(String),

    // --- DSP ---
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

    // --- Buttons ----
    SaveSettingsAndRestart,
    SettingsSaved(Result<String, Error>),

    SettingsFetched(Settings),
    SendSystemCommand(SystemCommand),
    SendUserCommand(UserCommand),
}

#[derive(Debug, Clone, EnumIter, PartialEq, Eq)]
pub enum FilterType {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Gain,
}

impl FilterType {
    fn to_string(&self) -> &str {
        match self {
            FilterType::Peaking => "Peaking",
            FilterType::LowShelf => "LowShelf",
            FilterType::HighShelf => "HighShelf",
            FilterType::LowPass => "LowPass",
            FilterType::HighPass => "HighPass",
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
    }
}

// ------ ------
//    Update
// ------ ------
#[allow(clippy::too_many_lines)]
pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
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

        Msg::InputMetadataMusicDirectoryChanged(value) => {
            model.settings.metadata_settings.music_directory = value;
        }

        Msg::InputAlsaCardChange(value) => {
            model.selected_audio_card_id = value.clone();
            model.settings.alsa_settings.output_device.card_id = value;
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
        Msg::SettingsFetched(sett) => {
            model.waiting_response = false;
            model.selected_audio_card_id = sett.alsa_settings.output_device.card_id.clone();
            model.settings = sett;
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
                model.settings.metadata_settings.music_directory.clone(),
                full_scan,
            ))));
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
            if let Some((_, filters)) = get_dsp_presets().get(index) {
                model.settings.rs_player_settings.dsp_settings.filters = filters.clone();
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
        _ => {}
    }
}

// ------ ------
//     View
// ------ ------
#[allow(clippy::too_many_lines)]
pub fn view(model: &Model) -> Node<Msg> {
    let settings = &model.settings;
    div![
        view_spinner_modal(model.waiting_response),
        // players
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "General"],
            div![
                C!["field", "is-grouped","is-grouped-multiline"],
                div![C!["control"],
                    label!["Audio interface", C!["label","has-text-white"]],
                    div![
                        C!["select"],
                        select![
                            option!["-- Select audio interface --"],
                            model
                            .settings
                            .alsa_settings
                            .available_audio_cards
                            .iter()
                            .map(|card| option![
                                IF!(model.settings.alsa_settings.output_device.card_id == card.id => attrs!(At::Selected => "")),
                                attrs! {At::Value => card.id},
                                card.name.clone()
                            ])],
                        input_ev(Ev::Change, |v| {
                            Msg::InputAlsaCardChange(v)
                        }),
                    ],
                ],
                div![C!["control"],
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
            ]
            ],
            view_rsp(&settings.rs_player_settings),
            view_metadata_storage(&model.settings.metadata_settings),
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
        ],

        // dsp
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "DSP Settings"],
            view_dsp_settings(&settings.rs_player_settings),
        ],

        // usb
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "RSPlayer firmware(control board) USB link"],
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
                        C!["button", "is-danger"],
                        "Power Off",
                        ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::SetFirmwarePower(false)))
                    ]
                ),
                IF!(model.settings.usb_settings.enabled =>
                    button![
                        C!["button", "is-success"],
                        "Power On",
                        ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::SetFirmwarePower(true)))
                    ]
                )
            ]
        ],

        // volume control
        IF!(!settings.usb_settings.enabled =>
            section![
                C!["section"],
                h1![C!["title","has-text-white"], "Volume control"],
                view_volume_control(model)
            ]
        ),

        // buttons
        div![
            C!["buttons"],
                button![
                    IF!(model.settings.validate().is_err() => attrs!{ At::Disabled => ""}),
                    C!["button"],
                    "Save & restart player",
                    ev(Ev::Click, |_| Msg::SaveSettingsAndRestart)
                ],
                button![
                    C!["button", "is-warning"],
                    "Restart player",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(
                        SystemCommand::RestartRSPlayer
                    ))
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

#[allow(clippy::too_many_lines)]
fn view_volume_control(model: &Model) -> Node<Msg> {
    let volume_settings = &model.settings.volume_ctrl_settings;
    let alsa_settings = &model.settings.alsa_settings;
    div![
        IF!(volume_settings.ctrl_device == VolumeCrtlType::Alsa =>
           div![
               C!["field"],
               label!["Alsa mixer:", C!["label","has-text-white"]],
               div![
                   C!["control"],
                   div![
                       C!["select"],
                           select![
                               option!["-- Select mixer --"],
                               alsa_settings.find_mixers_by_card_id(&model.selected_audio_card_id)
                               .iter()
                               .map(|pcmd|
                                   option![
                                       IF!(volume_settings.alsa_mixer_name.as_ref().is_some_and(|name| &pcmd.name == name) => attrs!(At::Selected => "")),
                                       attrs! {At::Value => pcmd.name.clone()},
                                       pcmd.name.clone()
                                   ]
                               ),
                               input_ev(Ev::Change, Msg::InputVolumeAlsaMixerChanged),
                           ],
                   ],
               ],
           ]
        ),
        div![
            C!["field"],
            label!["Volume step", C!["label", "has-text-white"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {
                        At::Value => volume_settings.volume_step
                        At::Type => "number"
                    },
                    input_ev(Ev::Input, move |value| { Msg::InputVolumeStepChanged(value) }),
                ],
            ],
        ],
    ]
}

fn view_metadata_storage(metadata_settings: &MetadataStoreSettings) -> Node<Msg> {
    div![
        label!["Music directory path", C!["label", "has-text-white"]],
        div![
            C!["field", "is-grouped"],
            div![
                C!["control", "is-expanded"],
                input![
                    C!["input"],
                    attrs! {
                        At::Value => metadata_settings.music_directory
                    },
                    input_ev(Ev::Input, move |value| {
                        Msg::InputMetadataMusicDirectoryChanged(value)
                    }),
                ],
            ],
            div![
                C!["control"],
                button![
                    C!["button"],
                    ev(Ev::Click, move |_| Msg::ClickRescanMetadataButton(false)),
                    "Update library"
                ]
            ],
            div![
                C!["control"],
                button![
                    C!["button", "is-warning"],
                    ev(Ev::Click, move |_| Msg::ClickRescanMetadataButton(true)),
                    "Full rescan"
                ]
            ],
        ]
    ]
}

fn view_rsp(rsp_settings: &RsPlayerSettings) -> Node<Msg> {
    div![
        C!["field"],
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
                "Set alsa buffer frame size (Experimental!)",
                attrs! {
                    At::For => "alsabufsize_cb"
                }
            ]
        ],
        IF!(rsp_settings.alsa_buffer_size.is_some()  =>
            div![
                C!["field"],
                div![
                    C!["control"],
                    input![
                        C!["input"],
                        attrs! {
                            At::Value => rsp_settings.alsa_buffer_size.unwrap_or(10000),
                            At::Type => "number"
                        },
                        input_ev(Ev::Input, move |value| { Msg::InputRspAlsaBufferSizeChange(value) }),
                    ],
                ],
            ]
        )
    ]
}

fn view_dsp_settings(rsp_settings: &RsPlayerSettings) -> Node<Msg> {
    div![
        C!["box", "has-background-dark"],
        div![
            C!["field", "mb-4"],
            label!["Load Preset", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![
                        option![attrs!{At::Value => ""}, "Select a preset..."],
                        get_dsp_presets().iter().enumerate().map(|(i, (name, _))| {
                            option![
                                attrs! {At::Value => i},
                                name
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
                C!["button", "is-primary"],
                "Add Filter",
                ev(Ev::Click, |_| Msg::DspAddFilter)
            ],
            button![
                C!["button", "is-danger"],
                "Remove All",
                ev(Ev::Click, |_| Msg::DspRemoveAllFilters)
            ],
            button![
                C!["button", "is-warning"],
                "Apply (Live)",
                ev(Ev::Click, |_| Msg::DspApplySettings)
            ],
            div![
                C!["file", "is-primary", "ml-2"],
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
}

fn view_filter(index: usize, filter_config: &FilterConfig) -> Node<Msg> {
    let filter = &filter_config.filter;
    let (current_type, freq, gain, q, slope) = match filter {
        DspFilter::Peaking { freq, gain, q } => (FilterType::Peaking, Some(*freq), Some(*gain), Some(*q), None),
        DspFilter::LowShelf { freq, gain, q, slope } => (FilterType::LowShelf, Some(*freq), Some(*gain), *q, *slope),
        DspFilter::HighShelf { freq, gain, q, slope } => (FilterType::HighShelf, Some(*freq), Some(*gain), *q, *slope),
        DspFilter::LowPass { freq, q } => (FilterType::LowPass, Some(*freq), None, Some(*q), None),
        DspFilter::HighPass { freq, q } => (FilterType::HighPass, Some(*freq), None, Some(*q), None),
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
                C!["button", "is-danger"],
                "Remove",
                ev(Ev::Click, move |_| Msg::DspRemoveFilter(index))
            ]
        ]
    ]
}


fn get_dsp_presets() -> Vec<(&'static str, Vec<FilterConfig>)> {
    vec![
        ("Flat", vec![]),
        ("Bass Boost", vec![
            FilterConfig { filter: DspFilter::LowShelf { freq: 100.0, gain: 4.0, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
        ("Bass Cut", vec![
            FilterConfig { filter: DspFilter::LowShelf { freq: 100.0, gain: -4.0, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
        ("Treble Boost", vec![
            FilterConfig { filter: DspFilter::HighShelf { freq: 10000.0, gain: 4.0, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
        ("Treble Cut", vec![
            FilterConfig { filter: DspFilter::HighShelf { freq: 10000.0, gain: -4.0, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
        ("Loudness", vec![
            FilterConfig { filter: DspFilter::LowShelf { freq: 100.0, gain: 3.0, q: Some(0.707), slope: None }, channels: vec![] },
            FilterConfig { filter: DspFilter::HighShelf { freq: 10000.0, gain: 3.0, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
        ("Vocal Clarity", vec![
            FilterConfig { filter: DspFilter::Peaking { freq: 2500.0, gain: 2.5, q: 1.0 }, channels: vec![] },
            FilterConfig { filter: DspFilter::LowShelf { freq: 200.0, gain: -1.5, q: Some(0.707), slope: None }, channels: vec![] },
        ]),
    ]
}

#[allow(clippy::future_not_send)]
async fn save_settings(settings: Settings, query: String) -> Result<String, Error> {
    let response = Request::post(format!("{API_SETTINGS_PATH}?{query}").as_str())
        .json(&settings)?
        .send()
        .await?;
    response.text().await
}
