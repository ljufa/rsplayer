use std::io;
use std::str;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use api_models::player::Command;


use failure::_core::time::Duration;

type ReadSocket = Arc<Mutex<dyn io::Read + Send>>;

const REMOTE_MAKER: &'static str = "dplayd";

pub fn start(tx: SyncSender<Command>, lirc_socket: ReadSocket) {
    tokio::task::spawn(async move {
        loop {
            let mut bytes = [0; 60];
            lirc_socket
                .lock()
                .unwrap()
                .read(&mut bytes)
                .expect("Failed to read lirc socket.");
            let result = str::from_utf8(&bytes).unwrap();
            let remote_maker = result.find(REMOTE_MAKER);
            if remote_maker.is_none() || result.len() < 18 {
                continue;
            }
            let end = remote_maker.unwrap();
            if end <= 18 {
                continue;
            }
            let key = &result[17..end - 1];
            match key {
                "00 KEY_PLAY" => {
                    tx.send(Command::Play).expect("Error");
                }
                "00 KEY_STOP" => {
                    tx.send(Command::Pause).expect("Error");
                }
                "00 KEY_NEXTSONG" => {
                    tx.send(Command::Next).expect("Error");
                }
                "00 KEY_PREVIOUSSONG" => {
                    tx.send(Command::Prev).expect("Error");
                }
                "00 KEY_EJECTCD" => {
                    tx.send(Command::ChangeAudioOutput).expect("Error");
                    std::thread::sleep(Duration::from_secs(1));
                }
                "05 KEY_POWER" => {
                    tx.send(Command::PowerOff).expect("Error");
                    std::thread::sleep(Duration::from_secs(10));
                }

                _ => {
                    let key_str = String::from(key);
                    if key_str.ends_with("KEY_DOWN") {
                        tx.send(Command::VolDown).expect("Error");
                    }
                    if key_str.ends_with("KEY_UP") {
                        tx.send(Command::VolUp).expect("Error");
                    }
                    if key_str.ends_with("KEY_NEXT") {
                        tx.send(Command::Rewind(5)).expect("Error");
                    }
                    if key_str.ends_with("KEY_PREVIOUS") {
                        tx.send(Command::Rewind(-5)).expect("Error");
                    }
                }
            }
        }
    });
    info!("IR command receiver started.");
}

#[cfg(test)]
mod tests {
    use std::io::Result;
    use std::sync::mpsc;

    use mockall::*;

    use super::*;

    mock! {
        UnixStream{}
        trait Read {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
        }
    }
    #[test]
    fn test_commands_received() {
        // prepare
        let (tx, rx) = mpsc::sync_channel(1);
        let mut mcs = MockUnixStream::new();
        let mut calls = 0;
        mcs.expect_read().returning(move |buf| {
            match calls {
                1 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_UP", buf);
                }
                2 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_DOWN", buf);
                }
                3 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_PLAY", buf);
                }
                4 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_STOP", buf);
                }
                5 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_NEXTSONG", buf);
                }
                6 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_PREVIOUSSONG", buf);
                }
                7 => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_MENU", buf);
                }
                _ => {}
            }
            calls = calls + 1;
            return Ok(1usize);
        });
        // act
        start(tx, Arc::new(Mutex::new(mcs)));
        // assert
        let mut iter = rx.iter();
        assert_eq!(iter.next().unwrap(), Command::VolUp);
        assert_eq!(iter.next().unwrap(), Command::VolDown);
        assert_eq!(iter.next().unwrap(), Command::Play);
        assert_eq!(iter.next().unwrap(), Command::Pause);
        assert_eq!(iter.next().unwrap(), Command::Next);
        assert_eq!(iter.next().unwrap(), Command::Prev);
        assert_eq!(iter.next().unwrap(), Command::TogglePlayer);
    }

    #[test]
    fn do_not_panic_if_maker_or_key_not_match() {
        // prepare
        let (tx, rx) = mpsc::sync_channel(1);
        let mut mcs = MockUnixStream::new();
        let mut count = 1;
        mcs.expect_read().returning(move |buf| {
            match count {
                1 => {
                    write_key_into_buffer_helper("1234523452341421", "00 KEY_DOWN", buf, "Samsung");
                }
                2 => {
                    write_key_into_buffer_ok_remote_maker("00", buf);
                }
                3 => {
                    write_key_into_buffer_helper(" ", "00 KEY_DOWN", buf, REMOTE_MAKER);
                }
                _ => {
                    write_key_into_buffer_ok_remote_maker("00 KEY_DOWN", buf);
                }
            }
            count = count + 1;
            Ok(1usize)
        });

        // act
        start(tx, Arc::new(Mutex::new(mcs)));
        // assert
        assert_eq!(rx.iter().next().unwrap(), Command::VolDown);
    }

    fn write_key_into_buffer_ok_remote_maker(key: &str, buf: &mut [u8]) -> Result<usize> {
        write_key_into_buffer_helper("1234523452341421", key, buf, REMOTE_MAKER)
    }

    fn write_key_into_buffer_helper(
        pref: &str,
        key: &str,
        buf: &mut [u8],
        remote_maker: &str,
    ) -> Result<usize> {
        let mut inp = String::new();
        inp.push_str(pref);
        inp.push_str(" ");
        inp.push_str(key);
        inp.push_str(" ");
        inp.push_str(remote_maker);
        let inp = inp.as_bytes();
        let mut i = 0;
        for byte in buf.iter_mut() {
            if i < inp.len() {
                *byte = inp[i];
                i = i + 1;
            }
        }
        Ok(inp.len())
    }
}
