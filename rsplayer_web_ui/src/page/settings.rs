
use api_models::{
    common::{MetadataCommand::RescanMetadata, SystemCommand, UserCommand, VolumeCrtlType},
    settings::{MetadataStoreSettings, RsPlayerSettings, Settings},
    validator::Validate,
};
use gloo_console::log;
use gloo_net::{http::Request, Error};
use seed::{attrs, button, div, h1, i, input, label, option, prelude::*, section, select, span, style, C, IF};

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

    // --- Buttons ----
    SaveSettingsAndRestart,
    SettingsSaved(Result<String, Error>),

    SettingsFetched(Settings),
    SendSystemCommand(SystemCommand),
    SendUserCommand(UserCommand),
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
            if model.settings.validate().is_ok() {
                let settings = model.settings.clone();
                orders.perform_cmd(async {
                    Msg::SettingsSaved(save_settings(settings, "reload=true".to_string()).await)
                });
                model.waiting_response = true;
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

#[allow(clippy::future_not_send)]
async fn save_settings(settings: Settings, query: String) -> Result<String, Error> {
    let response = Request::post(format!("{API_SETTINGS_PATH}?{query}").as_str())
        .json(&settings)?
        .send()
        .await?;
    response.text().await
}
