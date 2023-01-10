use anyhow::{format_err, Result};
use rsplayer_metadata::queue::PlaybackQueue;
use std::fs::File;
use std::path::Path;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

use log::{debug, error, info, warn};

use super::output::try_open;

pub struct SymphoniaPlayer {
    running: Arc<AtomicBool>,
    time: Arc<Mutex<(u64, u64)>>,
    queue: Arc<PlaybackQueue>,
    audio_device: String,
}

impl SymphoniaPlayer {
    pub fn new(queue: Arc<PlaybackQueue>, audio_device: String) -> Self {
        SymphoniaPlayer {
            running: Arc::new(AtomicBool::new(false)),
            time: Arc::new(Mutex::new((0, 0))),
            queue,
            audio_device,
        }
    }

    pub fn stop_playing(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn get_time(&self) -> (u64, u64) {
        *self.time.lock().unwrap()
    }

    pub fn play_all_in_queue(&self) -> JoinHandle<Result<PlaybackResult>> {
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let queue = self.queue.clone();
        let audio_device = self.audio_device.clone();
        let time = self.time.clone();
        thread::Builder::new()
            .name("player".to_string())
            .spawn(move || {
                let mut num_failed = 0;
                let loop_result = loop {
                    let Some(song) = queue.get_current_song() else {
                        break Ok(PlaybackResult::QueueFinished);
                    };
                    match play_file(&song.file, &running, &time, &audio_device) {
                        Ok(PlaybackResult::PlaybackStopped) => {
                            break Ok(PlaybackResult::PlaybackStopped);
                        }
                        Err(err) => {
                            error!("Failed to play file {}. Error: {:?}", song.file, err);
                            num_failed += 1;
                            if num_failed == 10 {
                                warn!("Number of failed songs is greater than 10. Aborting.");
                                running.store(false, Ordering::Relaxed);
                                break Err(anyhow::format_err!(
                                    "Number of failed songs is higher than 10. Aborting!"
                                ));
                            }
                        }
                        res => {
                            info!("Playback finished with result {:?}", res);
                            num_failed = 0;
                        }
                    }

                    if !queue.move_current_to_next_song() {
                        break Ok(PlaybackResult::QueueFinished);
                    }
                };
                loop_result
            })
            .expect("Failed to start player thread")
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum PlaybackResult {
    QueueFinished,
    SongFinished,
    PlaybackStopped,
}

fn play_file(
    path_str: &str,
    running: &Arc<AtomicBool>,
    time: &Arc<Mutex<(u64, u64)>>,
    audio_device: &str,
) -> Result<PlaybackResult> {
    debug!("Playing file {}", path_str);
    running.store(true, Ordering::Relaxed);
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

    let tb = track.codec_params.time_base.unwrap();
    let dur = track
        .codec_params
        .n_frames
        .map(|frames| track.codec_params.start_ts + frames)
        .unwrap();
    let dur = tb.calc_time(dur);

    let mut decoder = get_codecs().make(&track.codec_params, decode_opts)?;
    let mut audio_output = None;

    // Decode and play the packets belonging to the selected track.
    let loop_result = loop {
        if !running.load(Ordering::Relaxed) {
            debug!("Exit from play thread due to running flag change");
            break Ok(PlaybackResult::PlaybackStopped);
        }
        // Get the next packet from the format reader.
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break Ok(PlaybackResult::SongFinished);
            }
            Err(err) => break Err(err.into()),
        };

        *time.lock().unwrap() = (dur.seconds, tb.calc_time(packet.ts()).seconds);
        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(decoded_buff) => {
                // If the audio output is not open, try to open it.
                if audio_output.is_none() {
                    // Get the audio buffer specification. This is a description of the decoded
                    // audio buffer's sample format and sample rate.
                    let spec = *decoded_buff.spec();

                    // Get the capacity of the decoded buffer. Note that this is capacity, not
                    // length! The capacity of the decoded buffer is constant for the life of the
                    // decoder, but the length is not.
                    let duration = decoded_buff.capacity() as u64;

                    // Try to open the audio output.
                    let Ok(audio_out) = try_open(spec, duration, audio_device) else {
                            break Err(format_err!("Failed to open audio output {}", audio_device));
                        };
                    audio_output.replace(audio_out);
                } else {
                    // TODO: Check the audio spec. and duration hasn't changed.
                }
                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).

                if packet.ts() > 0 {
                    if let Some(audio_output) = audio_output.as_mut() {
                        _ = audio_output.write(decoded_buff);
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
        audio_output.flush();
    }
    debug!("Play finished with result {:?}", loop_result);
    loop_result
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
