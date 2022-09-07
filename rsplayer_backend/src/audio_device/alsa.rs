use std::collections::HashMap;

use alsa::device_name::HintIter;
use alsa::mixer::{SelemChannelId, SelemId};
use alsa::pcm::State;
use alsa::{Direction, Mixer};
use api_models::common::Volume;

use crate::common::Result;

use super::VolumeControlDevice;

const WAIT_TIME_MS: u64 = 10000;
const DELAY_MS: u64 = 100;

pub struct AlsaPcmCard {
    device_name: String,
}

impl AlsaPcmCard {
    pub fn new(device_name: String) -> Self {
        AlsaPcmCard { device_name }
    }
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
        Err(failure::format_err!(
            "Audio device [{}] remains locked after [{}]ms",
            self.device_name,
            &elapsed_time
        ))
    }
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
    mixer_name: String,
    mixer_idx: u32,
}

impl AlsaMixer {
    pub fn new(card_name: String, mixer_name: String, mixer_idx: u32) -> Result<Box<Self>> {
        Ok(Box::new(AlsaMixer {
            card_name,
            mixer_name,
            mixer_idx,
        }))
    }
}

impl VolumeControlDevice for AlsaMixer {
    fn vol_up(&self) -> Volume {
        let ev = self.get_vol();
        let nv = ev.current + ev.step;
        self.set_vol(nv)
    }

    fn vol_down(&self) -> Volume {
        let ev = self.get_vol();
        let nv = ev.current - ev.step;
        self.set_vol(nv)
    }

    fn get_vol(&self) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            let selem = mixer
                .find_selem(&SelemId::new(self.mixer_name.as_str(), self.mixer_idx))
                .unwrap();
            let (rmin, rmax) = selem.get_playback_volume_range();
            let mut channel = SelemChannelId::mono();
            for c in SelemChannelId::all().iter() {
                if selem.has_playback_channel(*c) {
                    channel = *c;
                    break;
                }
            }
            let old: i64 = selem.get_playback_volume(channel).unwrap();
            Volume {
                step: ALSA_MIXER_STEP,
                min: rmin,
                max: rmax,
                current: old,
            }
        } else {
            Volume::default()
        }
    }

    fn set_vol(&self, level: i64) -> Volume {
        if let Ok(mixer) = Mixer::new(self.card_name.as_str(), false) {
            let selem = mixer
                .find_selem(&SelemId::new(self.mixer_name.as_str(), self.mixer_idx))
                .unwrap();
            let (rmin, rmax) = selem.get_playback_volume_range();
            let mut channel = SelemChannelId::mono();
            for c in SelemChannelId::all().iter() {
                if selem.has_playback_channel(*c) {
                    channel = *c;
                    break;
                }
            }
            let old: i64 = selem.get_playback_volume(channel).unwrap();
            trace!("Changing volume of {} from {} to {}", channel, old, level);
            selem.set_playback_volume(channel, level).unwrap();
            Volume {
                step: ALSA_MIXER_STEP,
                min: rmin,
                max: rmax,
                current: level,
            }
        } else {
            Volume::default()
        }
    }
}

#[cfg(test)]
mod test {
    use alsa::{
        card,
        mixer::{Selem, SelemChannelId, SelemId},
        Mixer,
    };

    use super::AlsaPcmCard;

    #[test]
    fn print_mixer_of_cards() {
        for card in card::Iter::new().map(|c| c.unwrap()) {
            println!(
                "Card #{}: {} ({})",
                card.get_index(),
                card.get_name().unwrap(),
                card.get_longname().unwrap()
            );

            let mixer = Mixer::new(&format!("hw:{}", card.get_index()), false).unwrap();
            for selem in mixer.iter().filter_map(Selem::new) {
                let sid = selem.get_id();
                println!(
                    "\tMixer element {},{}:",
                    sid.get_name().unwrap(),
                    sid.get_index()
                );

                if selem.has_volume() {
                    print!("\t  Volume limits: ");
                    if selem.has_capture_volume() {
                        let (vmin, vmax) = selem.get_capture_volume_range();
                        let (mbmin, mbmax) = selem.get_capture_db_range();
                        print!("Capture = {} - {}", vmin, vmax);
                        print!(" ({} dB - {} dB)", mbmin.to_db(), mbmax.to_db());
                    }
                    if selem.has_playback_volume() {
                        let (vmin, vmax) = selem.get_playback_volume_range();
                        let (mbmin, mbmax) = selem.get_playback_db_range();
                        print!("Playback = {} - {}", vmin, vmax);
                        print!(" ({} dB - {} dB)", mbmin.to_db(), mbmax.to_db());
                    }
                    println!();
                }

                if selem.is_enumerated() {
                    print!("\t  Valid values: ");
                    for v in selem.iter_enum().unwrap() {
                        print!("{}, ", v.unwrap())
                    }
                    print!("\n\t  Current values: ");
                    for v in SelemChannelId::all()
                        .iter()
                        .filter_map(|&v| selem.get_enum_item(v).ok())
                    {
                        print!("{}, ", selem.get_enum_item_name(v).unwrap());
                    }
                    println!();
                }

                if selem.can_capture() {
                    print!("\t  Capture channels: ");
                    for channel in SelemChannelId::all() {
                        if selem.has_capture_channel(*channel) {
                            print!("{}, ", channel)
                        };
                    }
                    println!();
                    print!("\t  Capture volumes: ");
                    for channel in SelemChannelId::all() {
                        if selem.has_capture_channel(*channel) {
                            print!(
                                "{}: {} ({} dB), ",
                                channel,
                                match selem.get_capture_volume(*channel) {
                                    Ok(v) => format!("{}", v),
                                    Err(_) => "n/a".to_string(),
                                },
                                match selem.get_capture_vol_db(*channel) {
                                    Ok(v) => format!("{}", v.to_db()),
                                    Err(_) => "n/a".to_string(),
                                }
                            );
                        }
                    }
                    println!();
                }

                if selem.can_playback() {
                    print!("\t  Playback channels: ");
                    if selem.is_playback_mono() {
                        print!("Mono");
                    } else {
                        for channel in SelemChannelId::all() {
                            if selem.has_playback_channel(*channel) {
                                print!("{}, ", channel)
                            };
                        }
                    }
                    println!();
                    if selem.has_playback_volume() {
                        print!("\t  Playback volumes: ");
                        for channel in SelemChannelId::all() {
                            if selem.has_playback_channel(*channel) {
                                print!(
                                    "{}: {} / {}dB, ",
                                    channel,
                                    match selem.get_playback_volume(*channel) {
                                        Ok(v) => format!("{}", v),
                                        Err(_) => "n/a".to_string(),
                                    },
                                    match selem.get_playback_vol_db(*channel) {
                                        Ok(v) => format!("{}", v.to_db()),
                                        Err(_) => "n/a".to_string(),
                                    }
                                );
                            }
                        }
                        println!();
                    }
                }
            }
        }
    }

    #[test]
    fn get_and_set_playback_volume() {
        let mixer = Mixer::new("hw:0", false).unwrap();
        let selem = mixer.find_selem(&SelemId::new("Master", 0)).unwrap();

        let (rmin, rmax) = selem.get_playback_volume_range();
        let mut channel = SelemChannelId::mono();
        for c in SelemChannelId::all().iter() {
            if selem.has_playback_channel(*c) {
                channel = *c;
                break;
            }
        }
        println!(
            "Testing on {} with limits {}-{} on channel {}",
            selem.get_id().get_name().unwrap(),
            rmin,
            rmax,
            channel
        );

        let old: i64 = selem.get_playback_volume(channel).unwrap();
        let new: i64 = rmax / 2;
        assert_ne!(new, old);

        println!("Changing volume of {} from {} to {}", channel, old, new);
        selem.set_playback_volume(channel, new).unwrap();
        let result: i64 = selem.get_playback_volume(channel).unwrap();
        assert_eq!(new, result);

        // return volume to old value
        // selem.set_playback_volume(channel, old).unwrap();
        // result = selem.get_playback_volume(channel).unwrap();
        // assert_eq!(old, result);
    }
    #[test]
    fn list_devices() {
        AlsaPcmCard::get_all_cards()
            .iter()
            .for_each(|kv| println!("{} - {}", kv.0, kv.1));
    }
}
