use std::str::FromStr;

use api_models::{
    common::{
        CardMixer, FilterType, GainLevel, MetadataCommand::RescanMetadata, SystemCommand, UserCommand, VolumeCrtlType,
    },
    settings::{
        DacSettings, IRInputControlerSettings, MetadataStoreSettings, OLEDSettings, OutputSelectorSettings,
        RsPlayerSettings, Settings,
    },
};
use gloo_console::log;
use gloo_net::{http::Request, Error};
use seed::{attrs, button, div, h1, input, label, option, p, prelude::*, section, select, C, IF};
use strum::IntoEnumIterator;

use crate::view_spinner_modal;

const API_SETTINGS_PATH: &str = "/api/settings";

// ------ ------
//     Model

#[derive(Debug)]
pub struct Model {
    settings: Settings,
    selected_audio_card_index: i32,
    waiting_response: bool,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    // ---- on off toggles ----
    ToggleDacEnabled,
    ToggleIrEnabled,
    ToggleOledEnabled,
    ToggleOutputSelectorEnabled,
    ToggleRotaryVolume,
    ToggleResumePlayback,
    // ---- Input capture ----
    InputMetadataMusicDirectoryChanged(String),
    InputAlsaCardChange(i32),
    InputAlsaPcmChange(String),
    InputLircInputSocketPathChanged(String),
    InputLircRemoteMakerChanged(String),
    InputRotaryEventDevicePathChanged(String),
    InputVolumeStepChanged(String),
    InputVolumeCtrlDeviceChanged(VolumeCrtlType),
    InputRspBufferSizeChange(String),
    InputVolumeAlsaMixerChanged(String),
    InputDacAddressChanged(String),
    ClickRescanMetadataButton,

    InputAlsaDeviceChanged(String),

    InputDacFilterChanged(FilterType),
    InputDacGainLevelChanged(GainLevel),
    InputDacSoundSettingsChanged(String),

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
        selected_audio_card_index: -1,
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
            // todo: show modal wait window while server is restarting. use ws status.
            let settings = model.settings.clone();
            orders.perform_cmd(async { Msg::SettingsSaved(save_settings(settings, "reload=true".to_string()).await) });
            model.waiting_response = true;
        }
        Msg::ToggleDacEnabled => {
            model.settings.dac_settings.enabled = !model.settings.dac_settings.enabled;
        }
        Msg::ToggleIrEnabled => {
            model.settings.ir_control_settings.enabled = !model.settings.ir_control_settings.enabled;
        }
        Msg::ToggleOledEnabled => {
            model.settings.oled_settings.enabled = !model.settings.oled_settings.enabled;
        }
        Msg::ToggleOutputSelectorEnabled => {
            model.settings.output_selector_settings.enabled = !model.settings.output_selector_settings.enabled;
        }
        Msg::ToggleRotaryVolume => {
            model.settings.volume_ctrl_settings.rotary_enabled = !model.settings.volume_ctrl_settings.rotary_enabled;
        }
        Msg::ToggleResumePlayback => {
            model.settings.auto_resume_playback = !model.settings.auto_resume_playback;
        }

        Msg::InputMetadataMusicDirectoryChanged(value) => {
            model.settings.metadata_settings.music_directory = value;
        }

        Msg::InputAlsaCardChange(value) => {
            model.selected_audio_card_index = value;
        }
        Msg::InputAlsaPcmChange(value) => {
            model
                .settings
                .alsa_settings
                .set_output_device(model.selected_audio_card_index, &value);
        }
        Msg::InputDacFilterChanged(f) => {
            model.settings.dac_settings.filter = f;
        }
        Msg::InputDacGainLevelChanged(g) => {
            model.settings.dac_settings.gain = g;
        }
        Msg::InputLircInputSocketPathChanged(path) => {
            model.settings.ir_control_settings.input_socket_path = path;
        }
        Msg::InputLircRemoteMakerChanged(maker) => {
            model.settings.ir_control_settings.remote_maker = maker;
        }
        Msg::InputVolumeCtrlDeviceChanged(device) => {
            model.settings.volume_ctrl_settings.ctrl_device = device;
        }
        Msg::InputVolumeStepChanged(step) => {
            model.settings.volume_ctrl_settings.volume_step = step.parse::<u8>().unwrap_or_default();
        }
        Msg::InputVolumeAlsaMixerChanged(mixer) => {
            let pair: Vec<&str> = mixer.split(',').collect();
            model.settings.volume_ctrl_settings.alsa_mixer = Some(CardMixer {
                card_index: model.selected_audio_card_index,
                index: pair[0].parse().unwrap_or_default(),
                name: pair[1].to_owned(),
            });
        }

        Msg::InputRotaryEventDevicePathChanged(path) => {
            model.settings.volume_ctrl_settings.rotary_event_device_path = path;
        }
        Msg::InputRspBufferSizeChange(value) => {
            if let Ok(num) = value.parse::<usize>() {
                model.settings.rs_player_settings.buffer_size_mb = num;
            };
        }
        Msg::InputDacAddressChanged(value) => {
            if let Ok(num) = value.parse::<u16>() {
                model.settings.dac_settings.i2c_address = num;
            };
        }
        Msg::SettingsFetched(sett) => {
            model.waiting_response = false;
            model.settings = sett;
            model.selected_audio_card_index = model.settings.alsa_settings.output_device.card_index;
        }
        Msg::SettingsSaved(_saved) => {
            model.waiting_response = false;
        }
        Msg::ClickRescanMetadataButton => {
            let settings = model.settings.clone();
            orders.perform_cmd(async move {
                _ = save_settings(settings, "reload=false".to_string()).await;
            });
            orders.send_msg(Msg::SendUserCommand(UserCommand::Metadata(RescanMetadata(
                model.settings.metadata_settings.music_directory.clone(),
                false,
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
            view_rsp(&settings.rs_player_settings),
            div![
                C!["field"],
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
                                IF!(model.settings.alsa_settings.output_device.card_index == card.index => attrs!(At::Selected => "")),
                                attrs! {At::Value => card.index},
                                card.name.clone()
                            ])],
                        input_ev(Ev::Change, |v| {
                            let value = v.parse::<i32>().unwrap_or_default();
                            Msg::InputAlsaCardChange(value)
                        }),
                    ],
                ],
                p![C!["control"],"->"],
                div![C!["control"],
                    label!["PCM Device", C!["label","has-text-white"]],
                    div![
                        C!["select"],
                        select![
                            option!["-- Select pcm device --"],
                            model.settings.alsa_settings.find_pcms_by_card_index(model.selected_audio_card_index)
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
            view_metadata_storage(&model.settings.metadata_settings),
        ],
        // volume control
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "Volume control"],
            view_volume_control(model)
        ],
        // dac
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "Dac"],
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleDacEnabled),
                input![
                    C!["switch"],
                    attrs! {
                        At::Name => "dac_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.dac_settings.enabled.as_at_value(),
                    },
                ],
                label![
                    C!["label","has-text-white"],
                    "Enable DAC chip control?",
                    attrs! {
                        At::For => "dac_cb"
                    }
                ]
            ],
            IF!(settings.dac_settings.enabled => view_dac(&settings.dac_settings))
        ],
        // IR control
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "IR Control (Lirc)"],
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleIrEnabled),
                input![
                    C!["switch"],
                    attrs! {
                        At::Name => "ir_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.ir_control_settings.enabled.as_at_value(),
                    },
                ],
                label![
                    C!["label","has-text-white"],
                    "Enable Infra Red control with LIRC?",
                    attrs! {
                        At::For => "ir_cb"
                    }
                ]
            ],
            IF!(settings.ir_control_settings.enabled => view_ir_control(&settings.ir_control_settings))
        ],
        // oled display
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "OLED Display"],
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleOledEnabled),
                input![
                    C!["switch"],
                    attrs! {
                        At::Name => "oled_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.oled_settings.enabled.as_at_value(),
                    },
                ],
                label![C!["label","has-text-white"],
                    "Enable Oled Display?",
                    attrs! {
                        At::For => "oled_cb"
                    }
                ]
            ],
            IF!(settings.oled_settings.enabled => view_oled_display(&settings.oled_settings))
        ],
        // audio selector
        section![
            C!["section"],
            h1![C!["title","has-text-white"], "Audio output selector"],
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleOutputSelectorEnabled),
                input![
                    C!["switch"],
                    attrs! {
                        At::Name => "outsel_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.output_selector_settings.enabled.as_at_value(),
                    },
                ],
                label![C!["label","has-text-white"],
                    "Enable audio output selector (Headphone/Speakers)?",
                    attrs! {
                        At::For => "outsel_cb"
                    }
                ]
            ],
            IF!(settings.output_selector_settings.enabled => view_output_selector(&settings.output_selector_settings))
        ],
        // buttons
        div![
            C!["buttons"],
                button![
                    C!["button", "is-dark"],
                    "Save & restart player",
                    ev(Ev::Click, |_| Msg::SaveSettingsAndRestart)
                ],
                button![
                    C!["button", "is-dark"],
                    "Restart player",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(
                        SystemCommand::RestartRSPlayer
                    ))
                ],
                button![
                    C!["button", "is-dark"],
                    "Restart system",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(
                        SystemCommand::RestartSystem
                    ))
                ],
                button![
                    C!["button", "is-dark"],
                    "Shutdown system",
                    ev(Ev::Click, |_| Msg::SendSystemCommand(SystemCommand::PowerOff))
                ]
        ]
    ]
}

// ------ sub view functions ------
fn view_ir_control(ir_settings: &IRInputControlerSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["Remote maker", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![option![attrs!( At::Value => "Apple_A1156"), "Apple - A1156"]]
                ],
            ],
        ],
        div![
            C!["field"],
            label!["LIRC socket path", C!["label", "has-text-white"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {
                        At::Value => ir_settings.input_socket_path
                    },
                    input_ev(Ev::Input, move |value| { Msg::InputLircInputSocketPathChanged(value) }),
                ],
            ],
        ],
    ]
}

// use strum::IntoEnumIterator;
#[allow(clippy::too_many_lines)]
fn view_volume_control(model: &Model) -> Node<Msg> {
    let volume_settings = &model.settings.volume_ctrl_settings;
    let alsa_settings = &model.settings.alsa_settings;
    div![
        div![
            C!["field"],
            label!["Volume control device:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![
                        VolumeCrtlType::iter().map(|fs| {
                            let v: &str = fs.into();
                            option![
                                attrs!( At::Value => v),
                                IF!(volume_settings.ctrl_device == fs => attrs!(At::Selected => "")),
                                v
                            ]
                        }),
                        input_ev(Ev::Change, move |v| Msg::InputVolumeCtrlDeviceChanged(
                            VolumeCrtlType::from_str(v.as_str()).expect("msg")
                        )),
                    ],
                ],
            ],
        ],
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
                               alsa_settings.find_mixers_by_card_index(model.selected_audio_card_index)
                               .iter()
                               .map(|pcmd|
                                   option![
                                       IF!(volume_settings.alsa_mixer.as_ref().map_or(false, |f| pcmd.index == f.index && pcmd.name == f.name) => attrs!(At::Selected => "")),
                                       attrs! {At::Value => format!("{},{}", pcmd.index, pcmd.name )},
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
        div![
            C!["field"],
            ev(Ev::Click, |_| Msg::ToggleRotaryVolume),
            input![
                C!["switch"],
                attrs! {
                    At::Name => "rotary_cb"
                    At::Type => "checkbox"
                    At::Checked => volume_settings.rotary_enabled.as_at_value(),
                },
            ],
            label![
                C!["label", "has-text-white"],
                "Enable rotary encoder volume control",
                attrs! {
                    At::For => "rotary_cb"
                }
            ],
        ],
        IF!(volume_settings.rotary_enabled =>
            div![
                div![
                    C!["field"],
                    label!["Rotary encoder event device path", C!["label","has-text-white"]],
                    div![
                        C!["control"],
                        input![
                            C!["input"],
                            attrs! {
                                At::Value => volume_settings.rotary_event_device_path
                            },
                            input_ev(Ev::Input, move |value| {
                                Msg::InputRotaryEventDevicePathChanged(value)
                            }),
                        ],
                    ],
                ],
            ]
        )
    ]
}

fn view_oled_display(oled_settings: &OLEDSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["Display Model:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![option![attrs!( At::Value => "ST7920"), "ST7920 - 128x64"],],
                ],
            ],
        ],
        div![
            C!["field"],
            label!["SPI Device path:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                input![C!["input"], attrs! {At::Value => oled_settings.spi_device_path},],
            ],
        ],
    ]
}

fn view_output_selector(_out_settings: &OutputSelectorSettings) -> Node<Msg> {
    div![]
}

#[allow(clippy::too_many_lines)]
fn view_dac(dac_settings: &DacSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["DAC Chip:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![option![attrs!( At::Value => "AK4497"), "AK4497"],],
                ],
            ],
        ],
        div![
            C!["field"],
            label!["DAC I2C address:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {At::Value => dac_settings.i2c_address},
                ],
                input_ev(Ev::Input, move |value| { Msg::InputDacAddressChanged(value) }),
            ],
        ],
        div![
            C!["field"],
            label!["Digital filter:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![
                        FilterType::iter().map(|fs| {
                            let v: &str = fs.into();
                            option![
                                attrs!( At::Value => v),
                                IF!(dac_settings.filter == fs => attrs!(At::Selected => "")),
                                v
                            ]
                        }),
                        input_ev(Ev::Change, move |v| Msg::InputDacFilterChanged(
                            FilterType::from_str(v.as_str()).expect("msg")
                        )),
                    ],
                ],
            ],
        ],
        // gain level
        div![
            C!["field"],
            label!["Gain Level:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![
                        GainLevel::iter().map(|fs| {
                            let v: &str = fs.into();
                            option![
                                attrs!( At::Value => v),
                                IF!(dac_settings.gain == fs => attrs!(At::Selected => "")),
                                v
                            ]
                        }),
                        input_ev(Ev::Change, move |v| Msg::InputDacGainLevelChanged(
                            GainLevel::from_str(v.as_str()).expect("msg")
                        )),
                    ],
                ],
            ],
        ],
        // sound settings
        div![
            C!["field"],
            label!["Sound settings:", C!["label", "has-text-white"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![
                        option![
                            attrs!( At::Value => "1"),
                            IF!(dac_settings.sound_sett == 1 => attrs!(At::Selected => "")),
                            "Analog internal current, maximum (Setting1)"
                        ],
                        option![
                            attrs!( At::Value => "2"),
                            IF!(dac_settings.sound_sett == 2 => attrs!(At::Selected => "")),
                            " Analog internal current, minimum (Setting2)"
                        ],
                        option![
                            attrs!( At::Value => "3"),
                            IF!(dac_settings.sound_sett == 3 => attrs!(At::Selected => "")),
                            "Analog internal current, medium (Setting3)"
                        ],
                        option![
                            attrs!( At::Value => "4"),
                            IF!(dac_settings.sound_sett == 4 => attrs!(At::Selected => "")),
                            "Default (Setting 4)"
                        ],
                        option![
                            attrs!( At::Value => "5"),
                            IF!(dac_settings.sound_sett == 5 => attrs!(At::Selected => "")),
                            "High Sound Quality Mode (Setting 5)"
                        ],
                        input_ev(Ev::Change, Msg::InputDacSoundSettingsChanged),
                    ],
                ],
            ],
        ]
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
                    C!["button", "is-primary"],
                    ev(Ev::Click, move |_| Msg::ClickRescanMetadataButton),
                    "Update library"
                ]
            ],
        ]
    ]
}

fn view_rsp(rsp_settings: &RsPlayerSettings) -> Node<Msg> {
    div![
        C!["field"],
        label!["Input buffer size (in MB)", C!["label", "has-text-white"]],
        div![
            C!["control"],
            input![
                C!["input"],
                attrs! {At::Value => rsp_settings.buffer_size_mb, At::Type => "number"},
                input_ev(Ev::Input, move |value| { Msg::InputRspBufferSizeChange(value) }),
            ],
        ],
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
