use std::fs::File;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

use log::{debug, info, warn};

use super::output::{try_open, AudioOutput};
use super::PlayerCmd;

pub struct SymphoniaPlayer {
    running: Arc<Mutex<bool>>,
    rx_cmd: Receiver<PlayerCmd>,
}

impl SymphoniaPlayer {

    pub fn new(rx_cmd: Receiver<PlayerCmd>) -> Self {
        SymphoniaPlayer {
            running: Arc::new(Mutex::new(false)),
            rx_cmd,
        }
    }

    pub fn stop_playing(&self) {
        *self.running.lock().unwrap() = false;
    }
    
    pub fn is_playing(&self) -> bool{
        *self.running.lock().unwrap()
    }

    pub fn play_file(&mut self, path_str: String) -> JoinHandle<()> {
        *self.running.lock().unwrap() = true;
        let running = self.running.clone();
        thread::spawn(move || {
            let mut hint = Hint::new();
            let source = {
                let path = Path::new(&path_str);
                if let Some(extension) = path.extension() {
                    if let Some(extension_str) = extension.to_str() {
                        hint.with_extension(extension_str);
                    }
                }
                Box::new(File::open(path).unwrap())
            };
            // Probe the media source stream for metadata and get the format reader.
            if let Ok(probed) = symphonia::default::get_probe().format(
                            &hint,
                            MediaSourceStream::new(source, Default::default()),
                            &FormatOptions::default(),
                            &Default::default(),
                        ) {
                let mut reader: Box<dyn FormatReader> = probed.format;
                let decode_opts = &DecoderOptions::default();
                let track = first_supported_track(reader.tracks());

                let track_id = match track {
                    Some(track) => track.id,
                    _ => return,
                };

                // The audio output device.
                let mut audio_output = None;

                let reader: &mut Box<dyn FormatReader> = &mut reader;
                let audio_output: &mut Option<Box<dyn AudioOutput>> = &mut audio_output;
                // Get the selected track using the track ID.
                let track = match reader.tracks().iter().find(|track| track.id == track_id) {
                    Some(track) => track,
                    _ => return,
                };
                // Create a decoder for the track.
                let mut decoder = symphonia::default::get_codecs()
                    .make(&track.codec_params, decode_opts)
                    .unwrap();

                // Decode and play the packets belonging to the selected track.
                loop {
                    if !*running.lock().unwrap() {
                        debug!("Exit from play thread due to running flag change");
                        break;
                    }
                    // Get the next packet from the format reader.
                    let packet = match reader.next_packet() {
                        Ok(packet) => packet,
                        Err(symphonia::core::errors::Error::IoError(error))
                            if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                        {
                            break;
                        }
                        Err(err) => break,
                    };

                    // If the packet does not belong to the selected track, skip it.
                    if packet.track_id() != track_id {
                        continue;
                    }

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
                                audio_output.replace(try_open(spec, duration).unwrap());
                            } else {
                                // TODO: Check the audio spec. and duration hasn't changed.
                            }

                            // Write the decoded audio samples to the audio output if the presentation timestamp
                            // for the packet is >= the seeked position (0 if not seeking).
                            if packet.ts() > 0 {
                                if let Some(audio_output) = audio_output {
                                    audio_output.write(decoded).unwrap();
                                }
                            }
                        }
                        Err(Error::DecodeError(err)) => {
                            // Decode errors are not fatal. Print the error message and try to decode the next
                            // packet as usual.
                            warn!("decode error: {}", err);
                        }
                        Err(err) => break,
                    }
                };
                debug!("Play finished");

                // Flush the audio output to finish playing back any leftover samples.
                if let Some(audio_output) = audio_output.as_mut() {
                    audio_output.flush()
                }
                
            }
        })
    }
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
