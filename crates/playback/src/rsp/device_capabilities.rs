use cpal::traits::DeviceTrait;
use cpal::Device;

const STANDARD_RATES: &[u32] = &[192_000, 176_400, 96_000, 88_200, 48_000, 44_100, 32_000, 22_050, 16_000];

pub struct DeviceCapabilities {
    pub rate: Option<u32>,
    pub channels: Option<u16>,
}

impl DeviceCapabilities {
    pub fn query(device: &Device, source_rate: u32, source_channels: u16) -> Self {
        let rate = find_device_rate(device, source_rate);
        let channels = find_device_channels(device, source_channels);
        Self { rate, channels }
    }

    pub fn fallback_rates(_self: &Self, device: &Device, source_rate: u32) -> Vec<u32> {
        fallback_rate_candidates(device, source_rate)
    }
}

fn find_device_rate(device: &Device, source_rate: u32) -> Option<u32> {
    let Ok(configs) = device.supported_output_configs() else {
        return None;
    };

    let mut closest_rate: Option<u32> = None;
    let mut min_distance = u32::MAX;
    let mut best_multiple: Option<u32> = None;
    let mut best_factor = u32::MAX;

    for config in configs {
        if matches!(
            config.sample_format(),
            cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
        ) {
            continue;
        }

        let min_rate = config.min_sample_rate();
        let max_rate = config.max_sample_rate();

        if min_rate <= source_rate && max_rate >= source_rate {
            return None;
        }

        for &rate in &[min_rate, max_rate] {
            let distance = source_rate.abs_diff(rate);
            if distance < min_distance {
                min_distance = distance;
                closest_rate = Some(rate);
            }
            if rate > source_rate && rate % source_rate == 0 {
                let factor = rate / source_rate;
                if factor < best_factor {
                    best_factor = factor;
                    best_multiple = Some(rate);
                }
            }
        }
    }

    best_multiple.or(closest_rate)
}

fn find_device_channels(device: &Device, source_channels: u16) -> Option<u16> {
    let Ok(configs) = device.supported_output_configs() else {
        return None;
    };

    let mut supported = false;
    let mut closest: Option<u16> = None;
    let mut min_distance = u16::MAX;

    for config in configs {
        if matches!(
            config.sample_format(),
            cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
        ) {
            continue;
        }

        let ch = config.channels();
        if ch == source_channels {
            supported = true;
            break;
        }

        let distance = source_channels.abs_diff(ch);
        if distance < min_distance {
            min_distance = distance;
            closest = Some(ch);
        }
    }

    if supported {
        None
    } else {
        closest
    }
}

fn fallback_rate_candidates(device: &Device, source_rate: u32) -> Vec<u32> {
    let Ok(configs) = device.supported_output_configs() else {
        return vec![];
    };

    let ranges: Vec<(u32, u32)> = configs
        .filter(|c| {
            !matches!(
                c.sample_format(),
                cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
            )
        })
        .map(|c| (c.min_sample_rate(), c.max_sample_rate()))
        .collect();

    let in_any_range = |rate: u32| ranges.iter().any(|&(lo, hi)| rate >= lo && rate <= hi);

    let mut candidates: Vec<u32> = Vec::new();

    for factor in 2..=4u32 {
        let rate = source_rate.saturating_mul(factor);
        if rate != source_rate && in_any_range(rate) {
            candidates.push(rate);
        }
    }

    #[allow(clippy::tuple_array_conversions)]
    let mut boundaries: Vec<u32> = ranges
        .iter()
        .flat_map(|&(lo, hi)| [lo, hi])
        .filter(|&r| r != source_rate)
        .collect();
    boundaries.sort_by_key(|&r| source_rate.abs_diff(r));
    boundaries.dedup();
    candidates.extend(boundaries);

    let mut standard: Vec<u32> = STANDARD_RATES
        .iter()
        .copied()
        .filter(|&r| r != source_rate && in_any_range(r))
        .collect();
    standard.sort_by_key(|&r| source_rate.abs_diff(r));
    candidates.extend(standard);

    let mut seen = std::collections::HashSet::new();
    candidates.retain(|r| seen.insert(*r));
    candidates
}
