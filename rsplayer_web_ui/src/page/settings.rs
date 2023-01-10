use api_models::{
    common::{FilterType, GainLevel, PlayerType, SystemCommand, VolumeCrtlType},
    settings::{AlsaDeviceFormat, DacSettings, IRInputControlerSettings, LmsSettings, MetadataStoreSettings, MpdSettings, OLEDSettings, OutputSelectorSettings, Settings, VolumeControlSettings},
    spotify::SpotifyAccountInfo,
    validator::Validate,
};
use seed::{prelude::*, C, FutureExt, IF, attrs, button, div, empty, h1, i, input, label, log, option, p, section, select, span, style};
use std::str::FromStr;
use strum::IntoEnumIterator;

const API_SETTINGS_PATH: &str = "/api/settings";
const API_SPOTIFY_GET_AUTH_URL_PATH: &str = "/api/spotify/get-url";
const API_SPOTIFY_GET_ACCOUNT_INFO_PATH: &str = "/api/spotify/me";

// ------ ------
//     Model

#[derive(Debug) ]
pub struct Model {
    settings: Settings,
    waiting_response: bool,
    spotify_account_info: Option<SpotifyAccountInfo>,
}

#[derive(Debug)]
pub enum Msg {
    SelectActivePlayer(String),

    // ---- on off toggles ----
    ToggleDacEnabled,
    ToggleSpotifyEnabled,
    ToggleLmsEnabled,
    ToggleMpdEnabled,
    ToggleMpdOverrideConfig,
    ToggleIrEnabled,
    ToggleOledEnabled,
    ToggleOutputSelectorEnabled,
    ToggleRotaryVolume,

    // ---- Input capture ----
    InputMpdHostChange(String),
    InputMpdPortChange(u32),
    InputMetadataMusicDirectoryChanged(String),
    InputLMSHostChange,
    InputSpotifyDeviceNameChange(String),
    InputSpotifyUsernameChange(String),
    InputSpotifyPasswordChange(String),
    InputSpotifyAlsaDeviceFormatChanged(AlsaDeviceFormat),

    InputSpotifyDeveloperClientId(String),
    InputSpotifyDeveloperClientSecret(String),
    InputSpotifyAuthCallbackUrl(String),
    InputAlsaDeviceName(String),
    InputLircInputSocketPathChanged(String),
    InputLircRemoteMakerChanged(String),
    InputRotaryEventDevicePathChanged(String),
    InputVolumeStepChanged(String),
    InputVolumeCtrlDeviceChanged(VolumeCrtlType),

    ClickSpotifyAuthorizeButton,
    ClickSpotifyLogoutButton,
    ClickRescanMetadataButton,

    SpotifyAccountInfoFetched(Option<SpotifyAccountInfo>),
    SpotifyAuthorizationUrlFetched(String),

    InputAlsaDeviceChanged(String),

    InputDacFilterChanged(FilterType),
    InputDacGainLevelChanged(GainLevel),
    InputDacSoundSettingsChanged(String),

    // --- Buttons ----
    SaveSettingsAndRestart,
    SettingsSaved(fetch::Result<Settings>),

    RemoteConfiguration(Settings),
    SendCommand(SystemCommand),
}

// ------ ------
//     Init
// ------ ------
pub fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    log!("Settings Init called");
    orders.perform_cmd(async {
        let response = fetch(API_SETTINGS_PATH)
            .await
            .expect("Failed to get settings from backend");

        let sett = response
            .json::<Settings>()
            .await
            .expect("failed to deserialize to Configuration");
        Msg::RemoteConfiguration(sett)
    });
    orders.perform_cmd(async {
        Msg::SpotifyAccountInfoFetched(
            fetch(API_SPOTIFY_GET_ACCOUNT_INFO_PATH)
                .await
                .expect("")
                .json::<SpotifyAccountInfo>()
                .await
                .ok(),
        )
    });
    Model {
        settings: Settings::default(),
        waiting_response: false,
        spotify_account_info: None,
    }
}

// ------ ------
//    Update
// ------ ------

pub fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::SaveSettingsAndRestart => {
            // todo: show modal wait window while server is restarting. use ws status.
            let settings = model.settings.clone();
            orders.perform_cmd(async {
                Msg::SettingsSaved(save_settings(settings, "reload=true".to_string()).await)
            });
            model.waiting_response = true;
        }
        Msg::SelectActivePlayer(value) => {
            model.settings.active_player = PlayerType::from_str(value.as_str()).unwrap();
        }
        Msg::ToggleDacEnabled => {
            model.settings.dac_settings.enabled = !model.settings.dac_settings.enabled;
        }
        Msg::ToggleSpotifyEnabled => {
            model.settings.spotify_settings.enabled = !model.settings.spotify_settings.enabled;
        }
        Msg::ToggleLmsEnabled => {
            model.settings.lms_settings.enabled = !model.settings.lms_settings.enabled;
        }
        Msg::ToggleMpdEnabled => {
            model.settings.mpd_settings.enabled = !model.settings.mpd_settings.enabled;
        }
        Msg::ToggleIrEnabled => {
            model.settings.ir_control_settings.enabled =
                !model.settings.ir_control_settings.enabled;
        }
        Msg::ToggleOledEnabled => {
            model.settings.oled_settings.enabled = !model.settings.oled_settings.enabled;
        }
        Msg::ToggleOutputSelectorEnabled => {
            model.settings.output_selector_settings.enabled =
                !model.settings.output_selector_settings.enabled;
        }
        Msg::ToggleRotaryVolume => {
            model.settings.volume_ctrl_settings.rotary_enabled =
                !model.settings.volume_ctrl_settings.rotary_enabled;
        }
        Msg::ToggleMpdOverrideConfig => {
            model.settings.mpd_settings.override_external_configuration =
                !model.settings.mpd_settings.override_external_configuration;
        }
        Msg::InputMpdHostChange(value) => {
            model.settings.mpd_settings.server_host = value;
        }
        Msg::InputMpdPortChange(value) => {
            model.settings.mpd_settings.server_port = value;
        }
        Msg::InputMetadataMusicDirectoryChanged(value) => {
            model.settings.metadata_settings.music_directory = value;
        }
        Msg::InputLMSHostChange => {}
        Msg::InputSpotifyDeviceNameChange(value) => {
            model.settings.spotify_settings.device_name = value;
        }
        Msg::InputSpotifyUsernameChange(value) => {
            model.settings.spotify_settings.username = value;
        }
        Msg::InputSpotifyPasswordChange(value) => {
            model.settings.spotify_settings.password = value;
        }
        Msg::InputSpotifyDeveloperClientId(value) => {
            model.settings.spotify_settings.developer_client_id = value;
        }
        Msg::InputSpotifyDeveloperClientSecret(value) => {
            log!("Secret", value);
            model.settings.spotify_settings.developer_secret = value;
        }
        Msg::InputSpotifyAuthCallbackUrl(value) => {
            model.settings.spotify_settings.auth_callback_url = value;
        }
        Msg::InputSpotifyAlsaDeviceFormatChanged(value) => {
            model.settings.spotify_settings.alsa_device_format = value;
        }
        Msg::InputAlsaDeviceName(value) => {
            model.settings.alsa_settings.device_name = value;
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
            model.settings.volume_ctrl_settings.volume_step =
                step.parse::<u8>().unwrap_or_default();
        }
        Msg::InputRotaryEventDevicePathChanged(path) => {
            model.settings.volume_ctrl_settings.rotary_event_device_path = path;
        }
        Msg::ClickSpotifyAuthorizeButton => {
            let settings = model.settings.clone();
            orders.perform_cmd(async move {
                _ = save_settings(settings, "reload=false".to_string()).await;
                let url = fetch(API_SPOTIFY_GET_AUTH_URL_PATH)
                    .await
                    .expect("msg")
                    .text()
                    .await
                    .expect("msg");
                _ = seed::util::window().open_with_url(url.as_str());
            });
        }
        Msg::SpotifyAuthorizationUrlFetched(value) => {
            log!("Url fetched", value);
            // model.spotify_auth_url = Some(value);
        }
        Msg::SpotifyAccountInfoFetched(info) => {
            model.spotify_account_info = info;
        }
        Msg::RemoteConfiguration(sett) => {
            model.settings = sett;
        }
        Msg::SettingsSaved(saved) => {
            log!("Saved settings with result {}", saved);
            model.waiting_response = false;
        }
        Msg::ClickRescanMetadataButton => {
            let settings = model.settings.clone();
            orders.perform_cmd(async move {
                _ = save_settings(settings, "reload=false".to_string()).await;
            });
            orders.send_msg(Msg::SendCommand(SystemCommand::RescanMetadata(
                model.settings.metadata_settings.music_directory.clone(),
            )));
        }
        _ => {}
    }
}

async fn save_settings(settings: Settings, query: String) -> fetch::Result<Settings> {
    Request::new(format!("{API_SETTINGS_PATH}?{query}"))
        .method(Method::Post)
        .json(&settings)?
        .fetch()
        .await?
        .check_status()?
        .json::<Settings>()
        .await
}

// ------ ------
//     View
// ------ ------

pub fn view(model: &Model) -> Node<Msg> {
    div![
        // spinner
        div![
            C!["modal", IF!(model.waiting_response => "is-active")],
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
        ],
        view_settings(model)
    ]
}

// ------ configuration ------

fn view_settings(model: &Model) -> Node<Msg> {
    let settings = &model.settings;
    div![
        section![
            C!["section"],
            h1![C!["title"], "Players"],
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleMpdEnabled),
                input![
                    C!["control", "switch"],
                    attrs! {
                        At::Name => "mpd_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.mpd_settings.enabled.as_at_value(),
                    },
                ],
                label![
                    C!("label"),
                    "Enable Music Player Demon integration?",
                    attrs! {
                        At::For => "mpd_cb"
                    }
                ]
            ],
            IF!(settings.mpd_settings.enabled => view_mpd(&settings.mpd_settings)),
            div![
                C!["field"],
                ev(Ev::Click, |_| Msg::ToggleSpotifyEnabled),
                input![
                    C!["switch"],
                    attrs! {
                        At::Name => "spotify_cb"
                        At::Type => "checkbox"
                        At::Checked => settings.spotify_settings.enabled.as_at_value(),
                    },
                ],
                label![
                    C!["label"],
                    "Enable spotify integration (for premium Spotify accounts only)?",
                    attrs! {
                        At::For => "spotify_cb"
                    }
                ]
            ],
            IF!(settings.spotify_settings.enabled => view_spotify(model)),
            div![
                C!["field"],
                label!["Active player:", C!["label"]],
                div![
                    C!["select"],
                    select![
                        IF!(settings.spotify_settings.enabled =>
                        option![
                            attrs! {
                                At::Value => "SPF"
                            },
                            IF!(settings.active_player == PlayerType::SPF => attrs!(At::Selected => "")),
                            "Spotify"
                        ]),
                        IF!(settings.mpd_settings.enabled =>
                        option![
                            attrs! {At::Value => "MPD"},
                            IF!(settings.active_player == PlayerType::MPD => attrs!(At::Selected => "")),
                            "Music player daemon",
                        ]),
                        option![
                            attrs! {At::Value => "RSP"},
                            IF!(settings.active_player == PlayerType::RSP => attrs!(At::Selected => "")),
                            "RSPlayer - experimental",
                        ],
                        input_ev(Ev::Change, Msg::SelectActivePlayer),
                    ],
                ],
            ],
            div![
                C!["field"],
                label!["Audio device name", C!["label"]],
                div![
                    C!["select"],
                    select![model.settings
                        .alsa_settings
                        .available_alsa_pcm_devices
                        .iter()
                        .map(|d|
                            option![
                                IF!(model.settings.alsa_settings.device_name == *d.0 => attrs!(At::Selected => "")),
                                attrs! {At::Value => d.0},
                                d.1
                            ])
                        ],
                    input_ev(Ev::Change, Msg::InputAlsaDeviceName),
                ],
            ],
            view_metadata_storage(&model.settings.metadata_settings),
        ],
        section![
            C!["section"],
            h1![C!["title"], "Dac"],
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
                    "Enable DAC chip control?",
                    attrs! {
                        At::For => "dac_cb"
                    }
                ]
            ],
            IF!(settings.dac_settings.enabled => view_dac(&settings.dac_settings))
        ],
        section![
            C!["section"],
            h1![C!["title"], "IR Control (Lirc)"],
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
                    "Enable Infra Red control with LIRC?",
                    attrs! {
                        At::For => "ir_cb"
                    }
                ]
            ],
            IF!(settings.ir_control_settings.enabled => view_ir_control(&settings.ir_control_settings))
        ],
        section![
            C!["section"],
            h1![C!["title"], "Volume control"],
            view_volume_control(&settings.volume_ctrl_settings)
        ],
        section![
            C!["section"],
            h1![C!["title"], "OLED Display"],
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
                label![
                    "Enable Oled Display?",
                    attrs! {
                        At::For => "oled_cb"
                    }
                ]
            ],
            IF!(settings.oled_settings.enabled => view_oled_display(&settings.oled_settings))
        ],
        section![
            C!["section"],
            h1![C!["title"], "Audio output selector"],
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
                label![
                    "Enable audio output selector (Headphone/Speakers)?",
                    attrs! {
                        At::For => "outsel_cb"
                    }
                ]
            ],
            IF!(settings.output_selector_settings.enabled => view_output_selector(&settings.output_selector_settings))
        ],
        div![
            C!["field", "is-grouped"],
            div![
                C!("control"),
                button![
                    C!["button", "is-dark"],
                    "Save & restart player",
                    ev(Ev::Click, |_| Msg::SaveSettingsAndRestart)
                ]
            ],
            div![
                C!("control"),
                button![
                    C!["button", "is-dark"],
                    "Restart player",
                    ev(Ev::Click, |_| Msg::SendCommand(SystemCommand::RestartRSPlayer))
                ]
            ],
            div![
                C!("control"),
                button![
                    C!["button", "is-dark"],
                    "Restart system",
                    ev(Ev::Click, |_| Msg::SendCommand(SystemCommand::RestartSystem))
                ]
            ],
            div![
                C!("control"),
                button![
                    C!["button", "is-dark"],
                    "Shutdown system",
                    ev(Ev::Click, |_| Msg::SendCommand(SystemCommand::PowerOff))
                ]
            ]   ,
        ]
    ]
}
fn view_ir_control(ir_settings: &IRInputControlerSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["Remote maker", C!["label"]],
            div![
                C!["control"],
                div![
                    C!["select"],
                    select![option![
                        attrs!( At::Value => "Apple_A1156"),
                        "Apple - A1156"
                    ]]
                ],
            ],
        ],
        div![
            C!["field"],
            label!["LIRC socket path", C!["label"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {
                        At::Value => ir_settings.input_socket_path
                    },
                    input_ev(Ev::Input, move |value| {
                        Msg::InputLircInputSocketPathChanged(value)
                    }),
                ],
            ],
        ],
    ]
}
fn view_volume_control(volume_settings: &VolumeControlSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["Volume control device:", C!["label"]],
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

        div![
            C!["field"],
            label!["Volume step", C!["label"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {
                        At::Value => volume_settings.volume_step
                        At::Type => "number"
                    },
                    input_ev(Ev::Input, move |value| {
                        Msg::InputVolumeStepChanged(value)
                    }),
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
                    label!["Rotary encoder event device path", C!["label"]],
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
            label!["Display Model:", C!["label"]],
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
            label!["SPI Device path:", C!["label"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {At::Value => oled_settings.spi_device_path},
                ],
            ],
        ],
    ]
}
fn view_output_selector(_out_settings: &OutputSelectorSettings) -> Node<Msg> {
    div![]
}
fn view_dac(dac_settings: &DacSettings) -> Node<Msg> {
    div![
        div![
            C!["field"],
            label!["DAC Chip:", C!["label"]],
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
            label!["DAC I2C address:", C!["label"]],
            div![
                C!["control"],
                input![
                    C!["input"],
                    attrs! {At::Value => dac_settings.i2c_address, At::Type => "number"},
                ],
            ],
        ],
        div![
            C!["field"],
            label!["Digital filter:", C!["label"]],
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
        div![
            C!["field"],
            label!["Gain Level:", C!["label"]],
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
        div![
            C!["field"],
            label!["Sound settings:", C!["label"]],
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

fn view_validation_icon<Ms>(val: &impl api_models::validator::Validate, key: &str) -> Node<Ms> {
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

fn view_spotify(model: &Model) -> Node<Msg> {
    let spot_settings = &model.settings.spotify_settings;
    div![
        style! {
            St::PaddingBottom => "1.2rem"
        },
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Spotify connect device name", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                        input![C!["input"], attrs! {At::Value => spot_settings.device_name},],
                        input_ev(Ev::Input, move |value| {
                            Msg::InputSpotifyDeviceNameChange(value)
                        }),
                        view_validation_icon(spot_settings, "device_name")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Spotify username", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                    input![C!["input"], attrs! {At::Value => spot_settings.username},],
                    input_ev(Ev::Input, move |value| {
                        Msg::InputSpotifyUsernameChange(value)
                    }),
                    view_validation_icon(spot_settings, "username")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Spotify password", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                    input![C!["input"], attrs! {At::Value => spot_settings.password, At::Type => "password"}],
                    input_ev(Ev::Input, move |value| {
                        Msg::InputSpotifyPasswordChange(value)
                    }),
                    view_validation_icon(spot_settings, "password")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Developer client id", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                    input![
                        C!["input"],
                        attrs! {At::Value => spot_settings.developer_client_id},
                    ],
                    input_ev(Ev::Input, move |value| {
                        Msg::InputSpotifyDeveloperClientId(value)
                    }),
                    view_validation_icon(spot_settings, "developer_client_id")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Developer secret", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                    input![
                        C!["input"],
                        attrs! {At::Value => spot_settings.developer_secret, At::Type => "password"},
                    ],
                    input_ev(Ev::Input, move |value| {
                        Msg::InputSpotifyDeveloperClientSecret(value)
                    }),
                    view_validation_icon(spot_settings, "developer_secret")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Auth callback url", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control","has-icons-right"],
                    input![
                        C!["input"],
                        attrs! {At::Value => spot_settings.auth_callback_url},
                    ],
                    input_ev(Ev::Input, move |value| {
                        Msg::InputSpotifyAuthCallbackUrl(value)
                    }),
                    view_validation_icon(spot_settings, "auth_callback_url")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Connected Spotify account", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    IF!(spot_settings.validate().is_ok() && model.spotify_account_info.is_none() =>
                        button![C!["is-primary", "is-large"], ev(Ev::Click, move |_| Msg::ClickSpotifyAuthorizeButton), "Authorize"]
                    ),
                    if let Some(me) = &model.spotify_account_info {
                        p![
                            span![me.display_name.clone()],
                            span![me.email.clone()],
                            button![C!["is-primary", "is-large"], ev(Ev::Click, move |_| Msg::ClickSpotifyLogoutButton), "Logout"]
                        ]
                    } else {
                        empty!()
                    }
                ]
            ]
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Audio device format (for librespot)", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![C!["control"],
                    div![
                        C!["select"],
                        select![
                            AlsaDeviceFormat::iter().map(|fs| {
                                let v: &str = fs.into();
                                option![
                                    attrs!( At::Value => v),
                                    IF!(spot_settings.alsa_device_format == fs => attrs!(At::Selected => "")),
                                    v
                                ]
                            }),
                            input_ev(Ev::Change, move |v| Msg::InputSpotifyAlsaDeviceFormatChanged(
                                AlsaDeviceFormat::from_str(v.as_str()).expect("msg")
                            )),
                        ],
                    ],
                    ]
                ]
            ],
        ],

    ]
}
#[allow(dead_code)]
fn view_lms(lms_settings: &LmsSettings) -> Node<Msg> {
    div![
        style! {
            St::PaddingBottom => "1.2rem"
        },
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Logitech media server host", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    input![C!["input"], attrs! {At::Value => lms_settings.server_host},],
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Player port", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    input![C!["input"], attrs! {At::Value => lms_settings.server_port},],
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["CLI port", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    input![C!["input"], attrs! {At::Value => lms_settings.cli_port},],
                ]
            ],
        ],
    ]
}
fn view_metadata_storage(metadata_settings: &MetadataStoreSettings) -> Node<Msg> {
    div![
        C!["field"],
        label!["Music directory path", C!["label"]],
        input![
            C!["input"],
            attrs! {
                At::Value => metadata_settings.music_directory
            },
            input_ev(Ev::Input, move |value| {
                Msg::InputMetadataMusicDirectoryChanged(value)
            }),
        ],
        button![
            C!["is-primary", "is-large"],
            ev(Ev::Click, move |_| Msg::ClickRescanMetadataButton),
            "Rescan"
        ]
    ]
}
fn view_mpd(mpd_settings: &MpdSettings) -> Node<Msg> {
    div![
        style! {
            St::PaddingBottom => "1.2rem"
        },
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Music Player Daemon server host", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![
                        C!["control", "has-icons-right"],
                        input![
                            C!["input"],
                            attrs! {At::Value => mpd_settings.server_host},
                            input_ev(Ev::Input, move |value| { Msg::InputMpdHostChange(value) }),
                        ],
                        view_validation_icon(mpd_settings, "server_host")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            div![
                C!["field-label", "is-small"],
                label!["Client port", C!["label"]],
            ],
            div![
                C!["field-body"],
                div![
                    C!["field"],
                    div![
                        C!["control", "has-icons-right"],
                        input![
                            C!["input"],
                            attrs! {At::Value => mpd_settings.server_port, At::Type => "number"},
                            input_ev(Ev::Input, move |v| {
                                Msg::InputMpdPortChange(v.parse::<u32>().unwrap_or_default())
                            }),
                        ],
                        view_validation_icon(mpd_settings, "server_port")
                    ]
                ]
            ],
        ],
        div![
            C!["field", "is-horizontal"],
            ev(Ev::Click, |_| Msg::ToggleMpdOverrideConfig),
            div![
                C!["field-body"],
                div![
                    C!["control"],
                    input![
                        C!["switch"],
                        attrs! {
                            At::Name => "mpd_external_conf_cb"
                            At::Type => "checkbox"
                            At::Checked => mpd_settings.override_external_configuration.as_at_value(),
                        },
                    ],
                    label![
                        "Override existing mpd configuration (Music directory and Audio device)",
                        attrs! {
                            At::For => "mpd_external_conf_cb"
                        }
                    ]
                ],
            ]
        ],
    ]
}
