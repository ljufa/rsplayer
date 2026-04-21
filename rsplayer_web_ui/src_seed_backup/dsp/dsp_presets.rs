use api_models::settings::{DspFilter, FilterConfig};

pub struct DspPreset {
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
    pub filters: Vec<FilterConfig>,
}

pub fn get_dsp_presets() -> Vec<DspPreset> {
    vec![
        DspPreset {
            name: "Flat".to_string(),
            description: "No equalization (reset)".to_string(),
            filters: vec![],
        },
        DspPreset {
            name: "Bass Boost".to_string(),
            description: "Boosts low frequencies for a punchy sound".to_string(),
            filters: vec![FilterConfig {
                filter: DspFilter::LowShelf {
                    freq: 100.0,
                    q: Some(0.707),
                    slope: None,
                    gain: 6.0,
                },
                channels: vec![], // Global
            }],
        },
        DspPreset {
            name: "Vocal Boost".to_string(),
            description: "Enhances vocals and mid-range instruments".to_string(),
            filters: vec![
                FilterConfig {
                    filter: DspFilter::Peaking {
                        freq: 1000.0,
                        q: 1.0,
                        gain: 4.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::HighPass { freq: 200.0, q: 0.707 },
                    channels: vec![],
                },
            ],
        },
        DspPreset {
            name: "Treble Boost".to_string(),
            description: "Increases high frequencies for clarity".to_string(),
            filters: vec![FilterConfig {
                filter: DspFilter::HighShelf {
                    freq: 4000.0,
                    q: Some(0.707),
                    slope: None,
                    gain: 6.0,
                },
                channels: vec![],
            }],
        },
        DspPreset {
            name: "Rock".to_string(),
            description: "Classic 'V' shape: boosted bass and treble".to_string(),
            filters: vec![
                FilterConfig {
                    filter: DspFilter::LowShelf {
                        freq: 100.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 4.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::Peaking {
                        freq: 1000.0,
                        q: 1.0,
                        gain: -3.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::HighShelf {
                        freq: 4000.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 4.0,
                    },
                    channels: vec![],
                },
            ],
        },
        DspPreset {
            name: "Jazz".to_string(),
            description: "Warm lows and clear highs".to_string(),
            filters: vec![
                FilterConfig {
                    filter: DspFilter::LowShelf {
                        freq: 150.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 2.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::Peaking {
                        freq: 2500.0,
                        q: 2.0,
                        gain: 2.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::HighShelf {
                        freq: 10000.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 1.0,
                    },
                    channels: vec![],
                },
            ],
        },
        DspPreset {
            name: "Loudness".to_string(),
            description: "Compensates for low volume listening".to_string(),
            filters: vec![
                FilterConfig {
                    filter: DspFilter::LowShelf {
                        freq: 60.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 8.0,
                    },
                    channels: vec![],
                },
                FilterConfig {
                    filter: DspFilter::HighShelf {
                        freq: 8000.0,
                        q: Some(0.707),
                        slope: None,
                        gain: 5.0,
                    },
                    channels: vec![],
                },
            ],
        },
    ]
}
