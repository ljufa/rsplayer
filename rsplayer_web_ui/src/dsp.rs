use api_models::settings::{DspFilter, FilterConfig};
use gloo_console::warn;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct DspConfig {
    #[serde(default)]
    filters: HashMap<String, DspFilterDef>,
    #[serde(default)]
    pipeline: DspPipeline,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DspPipeline {
    List(Vec<DspPipelineStep>),
    Single(DspPipelineStep),
}

impl Default for DspPipeline {
    fn default() -> Self {
        DspPipeline::List(Vec::new())
    }
}

#[derive(Debug, Deserialize)]
struct DspPipelineStep {
    #[serde(rename = "type")]
    #[serde(default)]
    step_type: String,
    channel: Option<usize>,
    #[serde(default)]
    names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DspFilterDef {
    #[serde(rename = "type")]
    filter_type: String,
    parameters: serde_yaml::Value,
}

pub fn parse_dsp_config(yaml_content: &str) -> Vec<FilterConfig> {
    let config: DspConfig = match serde_yaml::from_str(yaml_content) {
        Ok(c) => c,
        Err(e) => {
            warn!(format!("Failed to parse YAML: {}", e));
            return Vec::new();
        }
    };

    let mut filters_config = Vec::new();

    let steps = match config.pipeline {
        DspPipeline::List(s) => s,
        DspPipeline::Single(s) => vec![s],
    };

    for step in steps {
        if !step.step_type.is_empty() && step.step_type != "Filter" {
            continue;
        }

        let channels = match step.channel {
            Some(ch) => vec![ch],
            None => vec![], // Global
        };

        for filter_name in step.names {
            if let Some(def) = config.filters.get(&filter_name) {
                if let Some(dsp_filter) = convert_filter(def) {
                    filters_config.push(FilterConfig {
                        filter: dsp_filter,
                        channels: channels.clone(),
                    });
                }
            } else {
                warn!(format!("Filter definition not found for name: {}", filter_name));
            }
        }
    }

    filters_config
}

fn convert_filter(def: &DspFilterDef) -> Option<DspFilter> {
    let params = &def.parameters;

    // Determine the actual filter type.
    // In CamillaDSP, it's often type: Biquad, parameters: { type: Peaking, ... }
    let actual_type = if def.filter_type == "Biquad" {
        params.get("type").and_then(|v| v.as_str()).unwrap_or("Peaking")
    } else {
        &def.filter_type
    };

    match actual_type {
        "Peaking" => {
            let freq = get_f64(params, "freq")?;
            let q = get_f64(params, "q")?;
            let gain = get_f64(params, "gain")?;
            Some(DspFilter::Peaking { freq, q, gain })
        }
        "LowShelf" => {
            let freq = get_f64(params, "freq")?;
            let q = get_f64(params, "q");
            let slope = get_f64(params, "slope");
            let gain = get_f64(params, "gain")?;
            Some(DspFilter::LowShelf { freq, q, slope, gain })
        }
        "HighShelf" => {
            let freq = get_f64(params, "freq")?;
            let q = get_f64(params, "q");
            let slope = get_f64(params, "slope");
            let gain = get_f64(params, "gain")?;
            Some(DspFilter::HighShelf { freq, q, slope, gain })
        }
        "LowPass" => {
            let freq = get_f64(params, "freq")?;
            let q = get_f64(params, "q").unwrap_or(0.707);
            Some(DspFilter::LowPass { freq, q })
        }
        "HighPass" => {
            let freq = get_f64(params, "freq")?;
            let q = get_f64(params, "q").unwrap_or(0.707);
            Some(DspFilter::HighPass { freq, q })
        }
        "Gain" => {
            let gain = get_f64(params, "gain")?;
            Some(DspFilter::Gain { gain })
        }
        _ => {
            warn!(format!("Unsupported filter type in import: {}", actual_type));
            None
        }
    }
}

fn get_f64(value: &serde_yaml::Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
}
