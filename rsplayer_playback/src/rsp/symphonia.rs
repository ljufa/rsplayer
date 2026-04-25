use std::time::Duration;

use anyhow::{format_err, Result};

use api_models::player::Song;
use api_models::state::{PlayerInfo, SongProgress, StateChangeEvent};
use log::{debug, warn};
use rsplayer_metadata::ape_bundle::ApeReader;
use rsplayer_metadata::dsd_bundle::{CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF};
use rsplayer_metadata::{build_codec_registry, build_probe};

use rsplayer_metadata::radio_meta::RadioMeta;
use symphonia::core::audio::Channels;
use symphonia::core::codecs::{
    audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO},
    CodecParameters,
};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use crate::rsp::alsa_output::AlsaOutput;
use crate::rsp::audio_source::{
    is_http_stream, probe_http_source, probe_local_file, resolve_ape_path, resolve_sacd_iso_path,
};
use crate::rsp::device_capabilities::DeviceCapabilities;
use crate::rsp::playback_config::PlaybackConfig;
use crate::rsp::playback_context::PlaybackContext;

use cpal::traits::{DeviceTrait, HostTrait};

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
    config: &PlaybackConfig,
    context: &mut PlaybackContext,
    track_loudness_lufs: Option<i32>,
    normalization_gain_db: Option<i32>,
) -> Result<PlaybackResult> {
    debug!("Playing file {path_str}");
    let mut hint = Hint::new();

    let is_seekable = !is_http_stream(path_str);

    // For APE files: open the file directly and construct ApeReader without
    // reading the entire file into memory. Only headers + seek table are read
    // upfront (~1KB); frame data is read on demand during playback.
    let ape_path = if is_http_stream(path_str) {
        None
    } else {
        resolve_ape_path(path_str, &config.music_dirs)
    };

    // For SACD ISO virtual paths (e.g. "album.iso#SACD_0002"): open the ISO directly
    // and construct a SacdIsoReader for the specified track index.
    let sacd_path = if is_http_stream(path_str) {
        None
    } else {
        resolve_sacd_iso_path(path_str, &config.music_dirs)
    };

    let mut radio_meta: Option<RadioMeta> = None;

    let mut reader: Box<dyn FormatReader + '_> = if let Some((iso_path, track_idx)) = sacd_path {
        debug!("SACD ISO: {} track {}", iso_path.display(), track_idx);
        let file = std::fs::File::open(&iso_path)
            .map_err(|e| format_err!("Failed to open SACD ISO {}: {e}", iso_path.display()))?;
        Box::new(
            rsplayer_metadata::sacd_bundle::SacdIsoReader::try_new_for_track(file, track_idx)
                .map_err(|e| format_err!("Failed to open SACD track: {e}"))?,
        )
    } else if let Some(ape_path) = ape_path {
        debug!("APE direct file path: {}", ape_path.display());
        // 1 MB buffer to amortize NFS round trips — default 8 KB causes
        // dozens of small reads per frame, starving the audio ring buffer on slow links.
        let file = std::io::BufReader::with_capacity(1024 * 1024, std::fs::File::open(&ape_path)?);
        Box::new(ApeReader::try_new_from_reader(file)?)
    } else {
        let (source, rm) = if is_http_stream(path_str) {
            probe_http_source(path_str, &mut hint, &context.changes_tx)?
        } else {
            probe_local_file(path_str, &config.music_dirs, &mut hint)?
        };
        radio_meta = rm;

        if let Some(rm) = &radio_meta {
            context
                .changes_tx
                .send(StateChangeEvent::CurrentSongEvent(Song {
                    title: rm.name.clone(),
                    album: rm.description.clone(),
                    genre: rm.genre.clone(),
                    file: rm.url.clone(),
                    image_url: rm.image_url.clone(),
                    ..Default::default()
                }))
                .ok();
        }

        build_probe()
            .probe(
                &hint,
                MediaSourceStream::new(source, symphonia::core::io::MediaSourceStreamOptions::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|_| format_err!("Media source probe failed"))?
    };

    let tracks = reader.tracks();
    let Some(track) = first_supported_track(tracks) else {
        return Err(format_err!("Invalid track"));
    };
    let track_id = track.id;
    let tb = track.time_base.unwrap_or_else(|| {
        TimeBase::new(
            std::num::NonZero::<u32>::new(1).expect("1 is non-zero"),
            std::num::NonZero::<u32>::new(1).expect("1 is non-zero"),
        )
    });
    let dur_ts = track
        .num_frames
        .map_or(1, |frames| track.start_ts.get().unsigned_abs().saturating_add(frames));
    let dur = tb.calc_time(Timestamp::new(dur_ts.cast_signed())).unwrap_or(Time::ZERO);

    let Some(CodecParameters::Audio(audio_params)) = track.codec_params.as_ref() else {
        return Err(format_err!("Invalid track codec params"));
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
    if let Err(e) = context.changes_tx.send(StateChangeEvent::PlayerInfoEvent(PlayerInfo {
        audio_format_bit: bps,
        audio_format_channels: chan_num,
        audio_format_rate: rate,
        codec: cd.map(|c| c.codec.info.short_name.to_uppercase()),
        track_loudness_lufs,
        normalization_gain_db,
    })) {
        warn!("Failed to send player info event: {e}");
    }

    let is_dsd = audio_params.codec == CODEC_TYPE_DSD_LSBF || audio_params.codec == CODEC_TYPE_DSD_MSBF;

    let mut decoder = codec_registry.make_audio_decoder(audio_params, &AudioDecoderOptions::default())?;
    let mut audio_output: Option<AlsaOutput> = None;
    let mut vu_meter = context.take_vu_meter();
    let mut last_current_time = 0;
    // Decode and play the packets belonging to the selected track.
    let loop_result = loop {
        if context.is_stopped() {
            debug!("Exit from play thread due to running flag change");
            break Ok(PlaybackResult::PlaybackStopped);
        }

        let skip_to = context.consume_skip_time();
        if is_seekable && skip_to > 0 {
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
            let _ = context.changes_tx.send(StateChangeEvent::SongTimeEvent(SongProgress {
                total_time: Duration::from_secs(dur.as_secs().unsigned_abs()),
                current_time: Duration::from_secs(current_time),
            }));
        }
        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(decoded_buff) => {
                // If the audio output is not open, try to open it.
                if audio_output.is_none() {
                    let spec = decoded_buff.spec().clone();
                    let duration = decoded_buff.capacity() as u64;
                    let spec_rate = spec.rate();
                    let spec_channels = spec.channels().count();

                    let host = cpal::default_host();
                    let device = if config.audio_device == "default" {
                        host.default_output_device()
                            .ok_or_else(|| format_err!("Default audio device not found!"))?
                    } else {
                        #[allow(deprecated)]
                        host.devices()?
                            .find(|d| d.name().unwrap_or_default() == config.audio_device)
                            .ok_or_else(|| format_err!("Device {} not found!", config.audio_device))?
                    };

                    #[allow(clippy::cast_possible_truncation)]
                    let caps = DeviceCapabilities::query(&device, spec_rate, spec_channels as u16);

                    let Ok(audio_out) = AlsaOutput::new(
                        spec,
                        duration,
                        &device,
                        &config.settings,
                        is_dsd,
                        context.dsp_handle.as_ref(),
                        vu_meter.take(),
                    ) else {
                        if caps.rate.is_none() {
                            let fallback_rates = DeviceCapabilities::fallback_rates(&caps, &device, spec_rate);
                            for fallback_rate in fallback_rates {
                                warn!("{spec_rate}Hz rejected, trying {fallback_rate}Hz");
                                if let Some(handle) = &context.dsp_handle {
                                    let ch = caps.channels.map_or(spec_channels, |c| c as usize);
                                    handle.rebuild(ch, fallback_rate as usize);
                                }
                                let spec_for_retry = decoded_buff.spec().clone();
                                if let Ok(audio_out) = AlsaOutput::new(
                                    spec_for_retry,
                                    duration,
                                    &device,
                                    &config.settings,
                                    is_dsd,
                                    context.dsp_handle.as_ref(),
                                    vu_meter.take(),
                                ) {
                                    debug!("Audio opened with fallback rate");
                                    audio_output.replace(audio_out);
                                    break;
                                }
                            }
                        }
                        break Err(format_err!("Failed to open audio output {}", config.audio_device));
                    };
                    debug!("Audio opened");

                    audio_output.replace(audio_out);
                }
                // Write the decoded audio samples to the audio output if the presentation timestamp

                let mut write_failed = false;
                if let Some(output) = audio_output.as_mut() {
                    if let Err(e) = output.write(decoded_buff) {
                        warn!("Audio output write error: {e}");
                        write_failed = true;
                    }
                }
                if write_failed {
                    audio_output = None;
                }
            }
            Err(Error::DecodeError(err)) => {
                warn!("decode error: {err}");
            }
            Err(err) => break Err(err.into()),
        }
    };

    if let Some(audio_output) = audio_output.as_mut() {
        audio_output.flush();
    }
    debug!("Play finished with result {loop_result:?}");
    loop_result
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks.iter().find(|t| {
        matches!(
            &t.codec_params,
            Some(CodecParameters::Audio(p)) if p.codec != CODEC_ID_NULL_AUDIO
        )
    })
}
