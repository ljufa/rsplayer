use std::ffi::CString;

use alsa::{card, Mixer};

use alsa::device_name::HintIter;
use alsa::mixer::{Selem, SelemChannelId, SelemId};
use alsa::pcm::State;

use anyhow::Result;
use api_models::common::{AudioCard, CardMixer, PcmOutputDevice, Volume};
use api_models::num_traits::ToPrimitive;
use log::debug;

use super::VolumeControlDevice;

#[allow(dead_code)]
const WAIT_TIME_MS: u64 = 10000;
#[allow(dead_code)]
const DELAY_MS: u64 = 100;

pub struct AlsaMixer {
    pub card_name: String,
    pub mixer_idx: u32,
    pub mixer_name: String,
}

pub struct AlsaPcmCard {
    device_name: String,
}

pub fn get_all_cards() -> Vec<AudioCard> {
    let mut result = vec![];
    for card in card::Iter::new().map(std::result::Result::unwrap) {
        let it = HintIter::new(Some(&card), &CString::new("pcm").unwrap()).unwrap();
        let mut pcm_devices = vec![];
        let card_name = card.get_name().unwrap_or_default();
        let card_index = card.get_index();
        for hint in it {
            pcm_devices.push(PcmOutputDevice {
                name: hint.name.unwrap_or_default(),
                description: hint.desc.map_or(String::new(), |dsc| dsc.replace('\n', " ")),
                card_id: card_name.clone(),
            });
        }
        result.push(AudioCard {
            id: card_name.clone(),
            index: card_index,
            name: card_name.clone(),
            description: card.get_longname().unwrap_or_default(),
            pcm_devices,
            mixers: get_card_mixers(&card_name, &card.get_index()),
        });
    }
    result
}

fn get_card_mixers(card_id: &str, card_idx: &i32) -> Vec<CardMixer> {
    let mixer_card_name = format!("hw:{card_idx}");
    let mut result = vec![];
    let Ok(mixer) = Mixer::new(&mixer_card_name, false) else {
        return result;
    };
    for selem in mixer.iter().filter_map(Selem::new) {
        let sid = selem.get_id();
        if selem.has_volume() && selem.has_playback_volume() {
            result.push(CardMixer {
                index: sid.get_index(),
                name: sid.get_name().unwrap_or("").to_owned(),
                card_id: card_id.to_string(),
            });
        }
    }
    result
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
            if let Ok(dev) = alsa::PCM::new(self.device_name.as_str(), alsa::Direction::Playback, false) {
                let status = dev.status().unwrap();
                debug!(
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
}
const ALSA_MIXER_STEP: u8 = 1;

impl AlsaMixer {
    pub fn new(card_idx: i32, mixer: Option<CardMixer>) -> Box<Self> {
        let m = mixer.unwrap_or_default();
        Box::new(AlsaMixer {
            card_name: format!("hw:{card_idx}"),
            mixer_idx: m.index,
            mixer_name: m.name,
        })
    }
}

impl VolumeControlDevice for AlsaMixer {
    fn vol_up(&mut self) -> Volume {
        let ev = self.get_vol();
        if let Some(nv) = ev.current.checked_add(ev.step) {
            if nv <= ev.max {
                self.set_vol(nv);
            }
        }
        ev
    }

    fn vol_down(&mut self) -> Volume {
        let ev = self.get_vol();
        if let Some(nv) = ev.current.checked_sub(ev.step) {
            if nv >= ev.min {
                self.set_vol(nv);
            }
        }
        ev
    }

    fn get_vol(&mut self) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            if let Some(selem) = mixer.find_selem(&SelemId::new(&self.mixer_name, self.mixer_idx)) {
                let (rmin, rmax) = selem.get_playback_volume_range();
                let mut channel = SelemChannelId::mono();
                for c in SelemChannelId::all() {
                    if selem.has_playback_channel(*c) {
                        channel = *c;
                        break;
                    }
                }
                let old: i64 = selem.get_playback_volume(channel).unwrap();
                return Volume {
                    step: ALSA_MIXER_STEP,
                    min: rmin.to_u8().unwrap_or(0),
                    max: rmax.to_u8().unwrap_or(0),
                    current: old.to_u8().unwrap_or(0),
                };
            }
        }
        Volume::default()
    }

    fn set_vol(&mut self, level: u8) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            if let Some(selem) = mixer.find_selem(&SelemId::new(&self.mixer_name, self.mixer_idx)) {
                let (rmin, rmax) = selem.get_playback_volume_range();
                for c in SelemChannelId::all() {
                    if selem.has_playback_channel(*c) {
                        selem
                            .set_playback_volume(*c, level.to_i64().unwrap_or_default())
                            .unwrap();
                    }
                }
                return Volume {
                    step: ALSA_MIXER_STEP,
                    min: rmin.to_u8().unwrap_or_default(),
                    max: rmax.to_u8().unwrap_or_default(),
                    current: level,
                };
            }
        }
        Volume::default()
    }
}
