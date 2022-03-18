use std::collections::HashMap;

use alsa::card;
use alsa::pcm::State;
use failure::Error;
use mockall::automock;

const WAIT_TIME_MS: u64 = 10000;
const DELAY_MS: u64 = 100;

pub struct AudioCard {
    device_name: String,
}


impl AudioCard {
    pub fn new(device_name: String) -> Self {
        AudioCard { device_name }
    }
    pub fn wait_unlock_audio_dev(&self) -> Result<(), Error> {
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
            elapsed_time = elapsed_time + DELAY_MS;
        }
        Err(failure::format_err!(
            "Audio device remains locked after [{}]ms",
            &elapsed_time
        ))
    }
    pub fn is_device_in_use(&self) -> bool {
        if let Ok(_) = alsa::PCM::new(self.device_name.as_str(), alsa::Direction::Playback, false) {
            return false;
        }
        return true;
    }

    // fix: not able to get current params because device is locked. read from file directly?
    pub fn get_hw_params(&self) -> Option<(u32, u32, String)> {
        let mut result = None;
        let dev = alsa::PCM::new(self.device_name.as_str(), alsa::Direction::Playback, true);
        if let Ok(dev) = dev {
            if let Ok(hwp) = dev.hw_params_current() {
                result = Some((
                    hwp.get_rate().map_or(0, |r| r),
                    hwp.get_channels().map_or(0, |c| c),
                    hwp.get_format()
                        .map_or(String::new(), |f| format!("{:?}", f).to_string()),
                ));
                debug!("hw_params from alsa {:?}", result);
            }
        }
        result
    }
}
pub fn get_all_cards() -> HashMap<String, String> {
    let mut result = HashMap::new();
    for a in card::Iter::new().map(|a| a.unwrap()) {
        result.insert(format!("hw:{}", a.get_index()), a.get_longname().unwrap());
    }
    result
}
