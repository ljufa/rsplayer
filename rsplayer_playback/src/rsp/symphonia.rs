use anyhow::{format_err, Result};
use std::fs::File;
use std::path::Path;

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

use log::{debug, warn};

use super::output::try_open;
use super::queue::PlaybackQueue;

pub struct SymphoniaPlayer {
    running: Arc<Mutex<bool>>,
    queue: Arc<Mutex<PlaybackQueue>>,
    audio_device: String,
}

impl SymphoniaPlayer {
    pub fn new(queue: Arc<Mutex<PlaybackQueue>>, audio_device: String) -> Self {
        SymphoniaPlayer {
            running: Arc::new(Mutex::new(false)),
            queue,
            audio_device,
        }
    }

    pub fn stop_playing(&self) {
        *self.running.lock().unwrap() = false;
    }

    pub fn is_playing(&self) -> bool {
        *self.running.lock().unwrap()
    }

    pub fn play_all_in_queue(&mut self) -> JoinHandle<Result<PlaybackResult>> {
        *self.running.lock().unwrap() = true;
        let running = self.running.clone();
        let queue = self.queue.clone();
        let audio_device = self.audio_device.clone();
        thread::spawn(move || {
            let loop_result = loop {
                let Some(song) = queue.lock().unwrap().get_current_song() else {
                        break Ok(PlaybackResult::QueueFinished);
                    };
                let play_result =
                    play_file(song.file.clone(), running.clone(), audio_device.clone())
                        .join()
                        .unwrap()?;
                if play_result == PlaybackResult::PlaybackStopped {
                    break Ok(play_result);
                }
                if !queue.lock().unwrap().move_current_to_next_song() {
                    break Ok(PlaybackResult::QueueFinished);
                }
            };
            loop_result
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum PlaybackResult {
    QueueFinished,
    SongFinished,
    PlaybackStopped,
}

fn play_file(
    path_str: String,
    running: Arc<Mutex<bool>>,
    audio_device: String,
) -> JoinHandle<Result<PlaybackResult>> {
    debug!("Playing file {}", path_str);
    *running.lock().unwrap() = true;

    thread::spawn(move || {
        let mut hint = Hint::new();
        let source = {
            let path = Path::new(&path_str);
            if let Some(extension) = path.extension() {
                if let Some(extension_str) = extension.to_str() {
                    hint.with_extension(extension_str);
                }
            }
            Box::new(File::open(path)?)
        };
        // Probe the media source stream for metadata and get the format reader.
        let Ok(probed) = get_probe().format(
            &hint,
            MediaSourceStream::new(source, MediaSourceStreamOptions::default()),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )else {
            return Err(format_err!("Media source probe failed"))
        };

        let mut reader: Box<dyn FormatReader> = probed.format;
        let decode_opts = &DecoderOptions::default();

        let Some(track) = first_supported_track(reader.tracks()) else {
            return Err(format_err!("Invalid track"));
        };
        let mut decoder = get_codecs().make(&track.codec_params, decode_opts)?;
        let mut audio_output = None;

        // Decode and play the packets belonging to the selected track.
        let loop_result = loop {
            if !*running.lock().unwrap() {
                debug!("Exit from play thread due to running flag change");
                break Ok(PlaybackResult::PlaybackStopped);
            }
            // Get the next packet from the format reader.
            let packet = match reader.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break Ok(PlaybackResult::SongFinished);
                }
                Err(err) => break Err(err.into()),
            };

            // Decode the packet into audio samples.
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    // If the audio output is not open, try to open it.
                    if audio_output.is_none() {
                        // Get the audio buffer specification. This is a description of the decoded
                        // audio buffer's sample format and sample rate.
                        let spec = *decoded.spec();

                        // Get the capacity of the decoded buffer. Note that this is capacity, not
                        // length! The capacity of the decoded buffer is constant for the life of the
                        // decoder, but the length is not.
                        let duration = decoded.capacity() as u64;

                        // Try to open the audio output.
                        audio_output
                            .replace(try_open(spec, duration, audio_device.clone()).unwrap());
                    } else {
                        // TODO: Check the audio spec. and duration hasn't changed.
                    }
                    // Write the decoded audio samples to the audio output if the presentation timestamp
                    // for the packet is >= the seeked position (0 if not seeking).
                    if packet.ts() > 0 {
                        if let Some(audio_output) = audio_output.as_mut() {
                            audio_output.write(decoded).unwrap();
                        }
                    }
                }
                Err(Error::DecodeError(err)) => {
                    // Decode errors are not fatal. Print the error message and try to decode the next
                    // packet as usual.
                    warn!("decode error: {}", err);
                }
                Err(err) => break Err(err.into()),
            }
        };
        // Flush the audio output to finish playing back any leftover samples.
        if let Some(audio_output) = audio_output.as_mut() {
            audio_output.flush()
        }
        debug!("Play finished with result {:?}", loop_result);
        loop_result
    })
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
