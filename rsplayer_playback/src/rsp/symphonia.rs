use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{format_err, Result};

use api_models::player::Song;
use api_models::settings::RsPlayerSettings;
use api_models::state::{PlayerInfo, SongProgress, StateChangeEvent};
use log::{debug, info, trace, warn};
use rsplayer_metadata::radio_meta::{self, RadioMeta};
use symphonia::core::audio::Channels;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};
use symphonia::default::{get_codecs, get_probe};
use tokio::sync::broadcast::Sender;

use crate::rsp::output::AudioOutput;

use super::output::try_open;

#[derive(Debug, Eq, PartialEq)]
pub enum PlaybackResult {
    QueueFinished,
    SongFinished,
    PlaybackStopped,
    PlaybackFailed,
}

unsafe impl Send for PlaybackResult {}

pub fn play_file(
    path_str: &str,
    stop_signal: &Arc<AtomicBool>,
    skip_to_time: &Arc<AtomicU16>,
    audio_device: &str,
    rsp_settings: &RsPlayerSettings,
    music_dir: &str,
    changes_tx: &Sender<StateChangeEvent>,
) -> Result<PlaybackResult> {
    debug!("Playing file {path_str}");
    let mut hint = Hint::new();
    let (media_source, radio_meta) = get_source(music_dir, path_str, &mut hint, changes_tx);
    let Ok(source) = media_source else {
        return Err(format_err!("Failed to get source: {:?}", media_source.err()));
    };
    if let Some(radio_meta) = &radio_meta {
        changes_tx
            .send(StateChangeEvent::CurrentSongEvent(Song {
                title: radio_meta.name.clone(),
                album: radio_meta.description.clone(),
                genre: radio_meta.genre.clone(),
                file: radio_meta.url.clone(),
                image_url: radio_meta.image_url.clone(),
                ..Default::default()
            }))
            .ok();
    }

    let is_seekable = source.is_seekable();
    // Probe the media source stream for metadata and get the format reader.
    let Ok(probed) = get_probe().format(
        &hint,
        MediaSourceStream::new(
            source,
            MediaSourceStreamOptions {
                buffer_len: (rsp_settings.input_stream_buffer_size_mb * 1024 * 1024).next_power_of_two(),
            },
        ),
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return Err(format_err!("Media source probe failed"));
    };

    let mut reader: Box<dyn FormatReader> = probed.format;

    let tracks = reader.tracks();
    let Some(track) = first_supported_track(tracks) else {
        return Err(format_err!("Invalid track"));
    };
    let track_id = track.id;
    let codec_parameters = &track.codec_params;
    let tb = codec_parameters.time_base.unwrap_or_else(|| TimeBase::new(1, 1));
    let dur = codec_parameters
        .n_frames
        .map_or(1, |frames| codec_parameters.start_ts + frames);
    let dur = tb.calc_time(dur);
    let mut rate = codec_parameters.sample_rate;
    let mut bps = codec_parameters.bits_per_sample;
    let mut chan_num = codec_parameters.channels.map(Channels::count);
    if let Some(radio_meta) = &radio_meta {
        rate = radio_meta.samplerate;
        chan_num = radio_meta.channels;
        bps = radio_meta.bitrate;
    }
    let cd = get_codecs().get_codec(codec_parameters.codec);
    changes_tx
        .send(StateChangeEvent::PlayerInfoEvent(PlayerInfo {
            audio_format_bit: bps,
            audio_format_channels: chan_num,
            audio_format_rate: rate,
            codec: cd.map(|c| c.long_name.to_string()),
        }))
        .expect("msg send failed");

    let is_dsd = codec_parameters.codec == CODEC_TYPE_DSD_LSBF || codec_parameters.codec == CODEC_TYPE_DSD_MSBF;

    let decode_opts = &DecoderOptions::default();
    let mut decoder = get_codecs().make(codec_parameters, decode_opts)?;
    let mut audio_output: Option<Box<dyn AudioOutput>> = None;
    let mut last_current_time = 0;
    // Decode and play the packets belonging to the selected track.
    let loop_result = loop {
        if stop_signal.load(Ordering::Relaxed) {
            debug!("Exit from play thread due to running flag change");
            break Ok(PlaybackResult::PlaybackStopped);
        }
        if is_seekable && skip_to_time.load(Ordering::Relaxed) > 0 {
            let skip_to = skip_to_time.swap(0, Ordering::Relaxed);
            debug!("Seeking to {skip_to}");
            let seek_result = reader.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::new(u64::from(skip_to), 0.0),
                    track_id: Some(track_id),
                },
            );
            if let Err(err) = seek_result {
                warn!("Seek failed: {err}");
            }
        }

        //  Get the next packet from the format reader.
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break Ok(PlaybackResult::SongFinished);
            }
            Err(err) => break Err(err.into()),
        };

        let current_time = tb.calc_time(packet.ts()).seconds;
        if current_time != last_current_time {
            last_current_time = current_time;
            changes_tx
                .send(StateChangeEvent::SongTimeEvent(SongProgress {
                    total_time: Duration::from_secs(dur.seconds),
                    current_time: Duration::from_secs(current_time),
                }))
                .expect("msg send failed");
        }
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
                    let Ok(audio_out) = try_open(spec, duration, audio_device, rsp_settings, is_dsd, changes_tx.clone()) else {
                        break Err(format_err!("Failed to open audio output {audio_device}"));
                    };
                    debug!("Audio opened");

                    audio_output.replace(audio_out);
                } else {
                    // TODO: Check the audio spec. and duration hasn't changed.
                }
                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).

                let mut write_failed = false;
                if packet.ts() > 0 {
                    if let Some(output) = audio_output.as_mut() {
                        trace!("Before audio write");
                        if let Err(e) = output.write(decoded_buff) {
                            warn!("Audio output write error: {e}");
                            write_failed = true;
                        }
                    }
                }
                if write_failed {
                    audio_output = None;
                }
            }
            Err(Error::DecodeError(err)) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                warn!("decode error: {err}");
            }
            Err(err) => break Err(err.into()),
        }
    };
    // Flush the audio output to finish playing back any leftover samples.
    if let Some(audio_output) = audio_output.as_mut() {
        audio_output.flush();
    }
    debug!("Play finished with result {loop_result:?}");
    loop_result
}

fn get_source(
    music_dir: &str,
    path_str: &str,
    hint: &mut Hint,
    changes_tx: &Sender<StateChangeEvent>,
) -> (Result<Box<dyn MediaSource>, anyhow::Error>, Option<RadioMeta>) {
    let mut radio_meta = None;
    let source = if path_str.starts_with("http") {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(5))
            .timeout_write(Duration::from_secs(5))
            .build();
        let Ok(resp) = agent.get(path_str).set("accept", "*/*").set("Icy-Metadata", "1").call() else {
            return (Err(format_err!("Failed to get url {path_str}")), None);
        };

        let status = resp.status();
        info!("response status code:{status} / status text:{}", resp.status_text());
        resp.headers_names()
            .iter()
            .for_each(|header| info!("{header} = {:?}", resp.header(header).unwrap_or("")));

        radio_meta = radio_meta::get_external_radio_meta(&agent, &resp);

        if status == 200 {
            let metaint_str = resp.header("icy-metaint");
            if let Some(metaint_val) = metaint_str.and_then(|s| s.parse::<usize>().ok()) {
                info!("ICY stream detected with metaint={metaint_val}");
                let reader = resp.into_reader();
                let icy_reader = radio_meta::IcyMetadataReader::new(
                    reader,
                    metaint_val,
                    changes_tx.clone(),
                    radio_meta.clone().unwrap(),
                );
                Box::new(ReadOnlySource::new(Box::new(icy_reader))) as Box<dyn MediaSource>
            } else {
                Box::new(ReadOnlySource::new(resp.into_reader())) as Box<dyn MediaSource>
            }
        } else {
            return (Err(format_err!("Invalid streaming url {path_str}")), None);
        }
    } else {
        let path = Path::new(music_dir).join(path_str);
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }
        if let Ok(p) = File::open(path) {
            Box::new(p)
        } else {
            return (Err(format_err!("Unable to open file: {path_str}")), None);
        }
    };
    (Ok(source), radio_meta)
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks.iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
