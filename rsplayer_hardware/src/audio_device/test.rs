#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery)]
mod test_alsa {
    use std::ffi::CString;

    use alsa::{
        card,
        device_name::HintIter,
        mixer::{Selem, SelemChannelId, SelemId},
        HCtl, Mixer,
    };

    #[test]
    fn test_set_volume() {
        for t in &["pcm", "ctl", "rawmidi", "timer", "seq", "hwdep"] {
            println!("{t} devices:");
            let i = HintIter::new(None, &CString::new(*t).unwrap()).unwrap();
            for a in i {
                if a.direction.is_none() {
                    println!("  {a:?}");
                }
            }
        }

        // let volume_ctrl = AlsaMixer::new("hw:0".to_string());

        // volume_ctrl.set_vol(80);
        // assert!(volume_ctrl.get_vol().current == 80);
    }

    #[test]
    fn print_mixer_of_cards() {
        for card in card::Iter::new().map(std::result::Result::unwrap) {
            println!(
                "[{}]:[{}]:[{}]",
                card.get_index(),
                card.get_name().unwrap(),
                card.get_longname().unwrap()
            );
            let mixer = Mixer::new(&format!("hw:{}", card.get_index()), false).unwrap();
            for selem in mixer.iter().filter_map(Selem::new) {
                let sid = selem.get_id();
                println!("\t{},{}:", sid.get_index(), sid.get_name().unwrap(),);

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
                        print!("{}, ", v.unwrap());
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
                            print!("{}, ", channel);
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
                                print!("{}, ", channel);
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
        for h in HCtl::new("hw:0", false).expect("msg").elem_iter() {
            println!("DD :{:?}", h.get_id());
        }
    }
}
