use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{format_err, Result};
use log::{debug, error, info, warn};
use symphonia::core::audio::Channels;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::{
    MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource,
};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;
use symphonia::default::{get_codecs, get_probe};

use rsplayer_metadata::queue::PlaybackQueue;

use super::output::try_open;

#[allow(clippy::type_complexity)]
pub struct SymphoniaPlayer {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    time: Arc<Mutex<(u64, u64)>>,
    codec_params: Arc<Mutex<(Option<u32>, Option<u32>, Option<usize>, Option<String>)>>,
    queue: Arc<PlaybackQueue>,
    audio_device: String,
    buffer_size_mb: usize,
    music_dir: String,
}

impl SymphoniaPlayer {
    pub fn new(
        queue: Arc<PlaybackQueue>,
        audio_device: String,
        buffer_size_mb: usize,
        music_dir: String,
    ) -> Self {
        SymphoniaPlayer {
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            time: Arc::new(Mutex::new((0, 0))),
            codec_params: Arc::new(Mutex::new((None, None, None, None))),
            queue,
            audio_device,
            buffer_size_mb,
            music_dir,
        }
    }

    pub fn stop_playing(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn pause_playing(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }
    pub fn un_pause_playing(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn get_time(&self) -> (u64, u64) {
        *self.time.lock().unwrap()
    }
    pub fn get_codec_params(&self) -> (Option<u32>, Option<u32>, Option<usize>, Option<String>) {
        self.codec_params
            .lock()
            .as_ref()
            .map(|t| (t.0, t.1, t.2, t.3.clone()))
            .unwrap()
    }

    pub fn play_all_in_queue(&self) -> JoinHandle<Result<PlaybackResult>> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let paused = self.paused.clone();
        let queue = self.queue.clone();
        let audio_device = self.audio_device.clone();
        let buffer_size = self.buffer_size_mb;
        let time = self.time.clone();
        let codec_params = self.codec_params.clone();
        let music_dir = self.music_dir.clone();
        thread::Builder::new()
            .name("player".to_string())
            .spawn(move || {
                let mut num_failed = 0;
                loop {
                    let Some(song) = queue.get_current_song() else {
                        running.store(false, Ordering::SeqCst);
                        break Ok(PlaybackResult::QueueFinished);
                    };
                    match play_file(
                        &song.file,
                        &running,
                        &paused,
                        &time,
                        &codec_params,
                        &audio_device,
                        buffer_size,
                        &music_dir,
                    ) {
                        Ok(PlaybackResult::PlaybackStopped) => {
                            running.store(false, Ordering::SeqCst);
                            break Ok(PlaybackResult::PlaybackStopped);
                        }
                        Err(err) => {
                            error!("Failed to play file {}. Error: {:?}", song.file, err);
                            num_failed += 1;
                            if num_failed == 10 {
                                warn!("Number of failed songs is greater than 10. Aborting.");
                                running.store(false, Ordering::SeqCst);
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
                }
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn play_file(
    path_str: &str,
    running: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    time: &Arc<Mutex<(u64, u64)>>,
    codec_params: &Arc<Mutex<(Option<u32>, Option<u32>, Option<usize>, Option<String>)>>,
    audio_device: &str,
    buffer_size_mb: usize,
    music_dir: &str,
) -> Result<PlaybackResult> {
    debug!("Playing file {}", path_str);
    running.store(true, Ordering::SeqCst);
    let mut hint = Hint::new();
    let source = get_source(music_dir, path_str, &mut hint)?;
    // Probe the media source stream for metadata and get the format reader.
    let Ok(probed) = get_probe().format(
        &hint,
        MediaSourceStream::new(source, MediaSourceStreamOptions{buffer_len: (buffer_size_mb * 1024 * 1024).next_power_of_two()}),
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )else {
        return Err(format_err!("Media source probe failed"));
    };

    let mut reader: Box<dyn FormatReader> = probed.format;
    let decode_opts = &DecoderOptions::default();

    let Some(track) = first_supported_track(reader.tracks()) else {
        return Err(format_err!("Invalid track"));
    };

    let codec_parameters = &track.codec_params;
    let tb = codec_parameters
        .time_base
        .unwrap_or_else(|| TimeBase::new(1, 1));
    let dur = codec_parameters
        .n_frames
        .map_or(1, |frames| codec_parameters.start_ts + frames);
    let dur = tb.calc_time(dur);

    let rate = codec_parameters.sample_rate;
    let bps = codec_parameters.bits_per_sample;
    let chan_num = codec_parameters.channels.map(Channels::count);
    let cd = symphonia::default::get_codecs().get_codec(codec_parameters.codec);
    *codec_params.lock().unwrap() = (rate, bps, chan_num, cd.map(|c| c.long_name.to_string()));
    let mut decoder = get_codecs().make(codec_parameters, decode_opts)?;
    let mut audio_output = None;

    // Decode and play the packets belonging to the selected track.
    let loop_result = loop {
        if !running.load(Ordering::SeqCst) {
            debug!("Exit from play thread due to running flag change");
            break Ok(PlaybackResult::PlaybackStopped);
        }
        if paused.load(Ordering::SeqCst) {
            debug!("Playing paused, going to sleep");
            thread::sleep(Duration::from_millis(300));
            continue;
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

fn get_source(
    music_dir: &str,
    path_str: &str,
    hint: &mut Hint,
) -> Result<Box<dyn MediaSource>, anyhow::Error> {
    let source = if path_str.starts_with("http") {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(5))
            .build();
        let resp = agent.get(path_str).set("accept", "*/*").call()?;
        let status = resp.status();
        info!(
            "response status code:{status} / status text:{}",
            resp.status_text()
        );
        resp.headers_names()
            .iter()
            .for_each(|header| debug!("{header} = {:?}", resp.header(header).unwrap_or("")));
        if status == 200 {
            Box::new(ReadOnlySource::new(resp.into_reader())) as Box<dyn MediaSource>
        } else {
            return Err(format_err!("Invalid streaming url {path_str}"));
        }
    } else {
        let path = Path::new(music_dir).join(path_str);
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }
        Box::new(File::open(path)?)
    };
    Ok(source)
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
