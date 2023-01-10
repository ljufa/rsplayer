use std::collections::HashMap;

use alsa::Direction;

use alsa::device_name::HintIter;
use alsa::mixer::{Selem, SelemChannelId};
use alsa::pcm::State;
use alsa::Mixer;
use api_models::common::Volume;

use anyhow::Result;

use super::VolumeControlDevice;

#[allow(dead_code)]
const WAIT_TIME_MS: u64 = 10000;
#[allow(dead_code)]
const DELAY_MS: u64 = 100;

pub struct AlsaPcmCard {
    device_name: String,
}

impl AlsaPcmCard {
    #[allow(dead_code)]
    pub const fn new(device_name: String) -> Self {
        AlsaPcmCard { device_name }
    }
    #[allow(dead_code)]
    pub fn wait_unlock_audio_dev(&self) -> Result<()> {
        let mut elapsed_time: u64 = 0;
        while elapsed_time < WAIT_TIME_MS {
            if let Ok(dev) =
                alsa::PCM::new(self.device_name.as_str(), alsa::Direction::Playback, false)
            {
                let status = dev.status().unwrap();
                trace!(
                    "Device status {:?} after elapsed time {}",
                    &status.get_state(),
                    &elapsed_time
                );
                if status.get_state() != State::Running {
                    return Ok(());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(DELAY_MS));
            elapsed_time += DELAY_MS;
        }
        Err(anyhow::format_err!(
            "Audio device [{}] remains locked after [{}]ms",
            self.device_name,
            &elapsed_time
        ))
    }

    #[allow(dead_code)]
    pub fn is_device_in_use(&self) -> bool {
        alsa::PCM::new(self.device_name.as_str(), alsa::Direction::Playback, false).is_err()
    }

    pub fn get_all_cards() -> HashMap<String, String> {
        let mut result = HashMap::new();
        let i = HintIter::new_str(None, "pcm").unwrap();
        for a in i {
            match a.direction {
                Some(Direction::Playback) | None => {
                    if let Some(name) = a.name {
                        let key = name.clone();
                        let mut value = name.clone();
                        if let Some(desc) = a.desc {
                            value = desc.replace('\n', " ");
                        }
                        result.insert(key, value);
                    }
                }
                _ => {}
            }
        }
        result
    }
}
const ALSA_MIXER_STEP: i64 = 1;
pub struct AlsaMixer {
    card_name: String,
}

impl AlsaMixer {
    pub fn new(card_name: String) -> Box<Self> {
        Box::new(AlsaMixer { card_name })
    }
}

impl VolumeControlDevice for AlsaMixer {
    fn vol_up(&self) -> Volume {
        let ev = self.get_vol();
        let nv = ev.current + ev.step;
        if nv <= ev.max {
            self.set_vol(nv)
        } else {
            ev
        }
    }

    fn vol_down(&self) -> Volume {
        let ev = self.get_vol();
        let nv = ev.current - ev.step;
        if nv >= ev.min {
            self.set_vol(nv)
        } else {
            ev
        }
    }

    fn get_vol(&self) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            if let Some(Some(selem)) = mixer.iter().next().map(Selem::new) {
                let (rmin, rmax) = selem.get_playback_volume_range();
                let mut channel = SelemChannelId::mono();
                for c in SelemChannelId::all().iter() {
                    if selem.has_playback_channel(*c) {
                        channel = *c;
                        break;
                    }
                }
                let old: i64 = selem.get_playback_volume(channel).unwrap();
                return Volume {
                    step: ALSA_MIXER_STEP,
                    min: rmin,
                    max: rmax,
                    current: old,
                };
            }
        }
        Volume::default()
    }

    fn set_vol(&self, level: i64) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            if let Some(Some(selem)) = mixer.iter().next().map(Selem::new) {
                let (rmin, rmax) = selem.get_playback_volume_range();
                for c in SelemChannelId::all().iter() {
                    if selem.has_playback_channel(*c) {
                        selem.set_playback_volume(*c, level).unwrap();
                    }
                }
                return Volume {
                    step: ALSA_MIXER_STEP,
                    min: rmin,
                    max: rmax,
                    current: level,
                };
            }
        }
        Volume::default()
    }
}

