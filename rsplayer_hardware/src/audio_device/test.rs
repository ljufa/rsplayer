#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery)]
mod test_alsa {
    use crate::audio_device::{alsa::AlsaMixer, VolumeControlDevice};
    use api_models::common::CardMixer;
    use crate::audio_device::alsa::get_all_cards;

    #[test]
    fn test_get_all_cards(){
        let cards = get_all_cards();
        cards.iter().for_each(|card| {
            println!("Card: {} - {}", card.name, card.description);
            card.pcm_devices.iter().for_each(|pcm| {
                println!("\tPCM: {}", pcm.name);
            });
            card.mixers.iter().for_each(|mixer| {
                println!("\tMixer: {}", mixer.name);
            });
        });
    }

    #[test]
    #[allow(unused)]
    fn test_new() {
        let mut mixer = Some(CardMixer{card_index: 0, index: 0, name: "Master".to_string()});
        let mut mix = AlsaMixer::new(0, mixer);
        assert_eq!(&mix.card_name, "hw:0");
        assert_eq!(mix.mixer_idx, 0u32);
        assert_eq!(&mix.mixer_name, "Master");
        
        mixer = Some(CardMixer{card_index: 1, index: 0, name: "Master".to_string()});
        mix = AlsaMixer::new(1, mixer);
        assert_eq!(&mix.card_name, "hw:1");
        
        mixer = Some(CardMixer{card_index: 10, index: 3, name: "Headphone L+R".to_string()});
        mix = AlsaMixer::new(10, mixer);
        assert_eq!(&mix.card_name, "hw:10");
        assert_eq!(mix.mixer_idx, 3u32);
        assert_eq!(&mix.mixer_name, "Headphone L+R");
    }

    #[test]
    #[allow(unused)]
    fn get_and_set_playback_volume() {
        let mix = AlsaMixer::new(0, Some(CardMixer { index: 0, name: "Master".to_string(), card_index: 0 }));
        let original_vol = mix.get_vol();
        assert_eq!(mix.set_vol(31).current, 31);
        assert_eq!(mix.get_vol().current, 31);
        assert_eq!(
            mix.set_vol(original_vol.current).current,
            original_vol.current
        );
        assert_eq!(mix.get_vol().current, original_vol.current);
    }

}
