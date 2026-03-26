use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{format_err, Result};

use api_models::player::Song;
use api_models::settings::RsPlayerSettings;
use api_models::state::{PlayerInfo, SongProgress, StateChangeEvent};
use log::{debug, info, warn};
use rsplayer_metadata::dsd_bundle::{build_codec_registry, build_probe, CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF};
use rsplayer_metadata::radio_meta::{self, RadioMeta};
use symphonia::core::audio::Channels;
use symphonia::core::codecs::{
    audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO},
    CodecParameters,
};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};
use tokio::sync::broadcast::Sender;

use crate::rsp::alsa_output::AlsaOutput;
use crate::rsp::vumeter::VUMeter;
use rsplayer_dsp::DspHandle;

#[derive(Debug, Eq, PartialEq)]
pub enum PlaybackResult {
    QueueFinished,
    SongFinished,
    PlaybackStopped,
    PlaybackFailed,
}

unsafe impl Send for PlaybackResult {}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn play_file(
    path_str: &str,
    stop_signal: &Arc<AtomicBool>,
    skip_to_time: &Arc<AtomicU16>,
    audio_device: &str,
    rsp_settings: &RsPlayerSettings,
    music_dirs: &[String],
    changes_tx: &Sender<StateChangeEvent>,
    dsp_handle: Option<&DspHandle>,
    vu_meter: Option<VUMeter>,
    track_loudness_lufs: Option<i32>,
    normalization_gain_db: Option<i32>,
) -> Result<PlaybackResult> {
    debug!("Playing file {path_str}");
    let mut hint = Hint::new();
    let (media_source, radio_meta) = get_source(music_dirs, path_str, &mut hint, changes_tx);
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
    // For non-seekable streams (HTTP), cap the buffer at 256KB.  The large
    // configured buffer (often 10+ MB) is designed for local file I/O.
    // For streams, Symphonia keeps ALL probe data in memory until a format
    // is identified, so a huge buffer means the probe can block for tens of
    // seconds on unsupported formats before giving up.
    let buffer_len = if is_seekable {
        (rsp_settings.input_stream_buffer_size_mb * 1024 * 1024).next_power_of_two()
    } else {
        (256 * 1024).min((rsp_settings.input_stream_buffer_size_mb * 1024 * 1024).next_power_of_two())
    };
    // Probe the media source stream for metadata and get the format reader.
    let Ok(mut reader) = build_probe().probe(
        &hint,
        MediaSourceStream::new(source, MediaSourceStreamOptions { buffer_len }),
        FormatOptions::default(),
        MetadataOptions::default(),
    ) else {
        return Err(format_err!("Media source probe failed"));
    };

    let tracks = reader.tracks();
    let Some(track) = first_supported_track(tracks) else {
        return Err(format_err!("Invalid track"));
    };
    let track_id = track.id;
    let tb = track
        .time_base
        .unwrap_or_else(|| TimeBase::new(std::num::NonZero::new(1).unwrap(), std::num::NonZero::new(1).unwrap()));
    let dur_ts = track
        .num_frames
        .map_or(1, |frames| track.start_ts.get().unsigned_abs().saturating_add(frames));
    let dur = tb.calc_time(Timestamp::new(dur_ts as i64)).unwrap_or(Time::ZERO);

    let audio_params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(p)) => p,
        _ => return Err(format_err!("Invalid track codec params")),
    };

    let mut rate = audio_params.sample_rate;
    let mut bps = audio_params.bits_per_sample;
    let mut chan_num = audio_params.channels.as_ref().map(Channels::count);
    if let Some(radio_meta) = &radio_meta {
        rate = radio_meta.samplerate;
        chan_num = radio_meta.channels;
        bps = radio_meta.bitrate;
    }
    let codec_registry = build_codec_registry();
    let cd = codec_registry.get_audio_decoder(audio_params.codec);
    changes_tx
        .send(StateChangeEvent::PlayerInfoEvent(PlayerInfo {
            audio_format_bit: bps,
            audio_format_channels: chan_num,
            audio_format_rate: rate,
            codec: cd.map(|c| c.codec.info.short_name.to_uppercase()),
            track_loudness_lufs,
            normalization_gain_db,
        }))
        .expect("msg send failed");

    let is_dsd = audio_params.codec == CODEC_TYPE_DSD_LSBF || audio_params.codec == CODEC_TYPE_DSD_MSBF;

    let mut decoder = codec_registry.make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;
    let mut audio_output: Option<AlsaOutput> = None;
    let mut vu_meter = vu_meter;
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
                    time: Time::try_new(i64::from(skip_to), 0).unwrap_or(Time::ZERO),
                    track_id: Some(track_id),
                },
            );
            if let Err(err) = seek_result {
                warn!("Seek failed: {err}");
            }
        }

        //  Get the next packet from the format reader.
        let packet = match reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break Ok(PlaybackResult::SongFinished),
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break Ok(PlaybackResult::SongFinished);
            }
            Err(err) => break Err(err.into()),
        };

        let current_time = tb.calc_time(packet.pts()).map_or(0, |t| t.as_secs().unsigned_abs());
        if current_time != last_current_time {
            last_current_time = current_time;
            changes_tx
                .send(StateChangeEvent::SongTimeEvent(SongProgress {
                    total_time: Duration::from_secs(dur.as_secs().unsigned_abs()),
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
                    let spec = decoded_buff.spec().clone();

                    // Get the capacity of the decoded buffer. Note that this is capacity, not
                    // length! The capacity of the decoded buffer is constant for the life of the
                    // decoder, but the length is not.
                    let duration = decoded_buff.capacity() as u64;

                    // Try to open the audio output.
                    let Ok(audio_out) = AlsaOutput::new(
                        spec,
                        duration,
                        audio_device,
                        rsp_settings,
                        is_dsd,
                        dsp_handle,
                        vu_meter.take(),
                    ) else {
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
                if packet.pts() > Timestamp::new(0) {
                    if let Some(output) = audio_output.as_mut() {
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
    music_dirs: &[String],
    path_str: &str,
    hint: &mut Hint,
    changes_tx: &Sender<StateChangeEvent>,
) -> (Result<Box<dyn MediaSource>, anyhow::Error>, Option<RadioMeta>) {
    let mut radio_meta = None;
    let source = if path_str.starts_with("http") {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(3))
            .timeout_write(Duration::from_secs(3))
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

        // Set format hint from content-type so Symphonia probes the right
        // reader first (or fails fast for unsupported formats).
        if let Some(ct) = resp.header("content-type") {
            let ext = match ct {
                "audio/mpeg" => Some("mp3"),
                "audio/aac" | "audio/aacp" | "audio/x-aac" => Some("aac"),
                "audio/ogg" | "application/ogg" => Some("ogg"),
                "audio/flac" | "audio/x-flac" => Some("flac"),
                "audio/wav" | "audio/x-wav" => Some("wav"),
                "audio/mp4" | "audio/x-m4a" => Some("m4a"),
                _ => None,
            };
            if let Some(ext) = ext {
                hint.with_extension(ext);
            }
        }

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
        // Try each music directory to find the file
        let mut found = None;
        for dir in music_dirs {
            let path = Path::new(dir).join(path_str);
            if let Some(extension) = path.extension() {
                if let Some(extension_str) = extension.to_str() {
                    hint.with_extension(extension_str);
                }
            }
            if let Ok(p) = File::open(&path) {
                found = Some(Box::new(p) as Box<dyn MediaSource>);
                break;
            }
        }
        if let Some(f) = found {
            f
        } else {
            return (Err(format_err!("Unable to open file: {path_str}")), None);
        }
    };
    (Ok(source), radio_meta)
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks.iter().find(|t| {
        matches!(
            &t.codec_params,
            Some(CodecParameters::Audio(p)) if p.codec != CODEC_ID_NULL_AUDIO
        )
    })
}
