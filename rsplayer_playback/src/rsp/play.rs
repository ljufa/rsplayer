// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]
// Justification: Fields on DecoderOptions and FormatOptions may change at any time, but
// symphonia-play doesn't want to be updated every time those fields change, therefore always fill
// in the remaining fields with default values.
#![allow(clippy::needless_update)]

use std::fs::File;
use std::io::Write;
use std::path::Path;

use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::{Cue, FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::{ColorMode, MetadataOptions, MetadataRevision, Tag, Value, Visual};
use symphonia::core::probe::{Hint, ProbeResult};
use symphonia::core::units::{Time, TimeBase};

use log::{debug, error, info, warn};

use super::output::{try_open, AudioOutput};

pub fn play_file(path_str: &str) -> Result<()> {
    let mut hint = Hint::new();
    let source = {
        let path = Path::new(path_str);
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }
        Box::new(File::open(path)?)
    };
    let mss = MediaSourceStream::new(source, Default::default());
    let format_opts = FormatOptions::default();
    let metadata_opts: MetadataOptions = Default::default();

    // Probe the media source stream for metadata and get the format reader.
    match symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
        Ok(probed) => {
            // If present, parse the seek argument.
            // let seek_time = matches.value_of("seek").map(|p| p.parse::<f64>().unwrap_or(0.0));

            // Set the decoder options.
            let decode_opts = DecoderOptions::default();

            // Play it!
            play(probed.format, None, None, &decode_opts)
        }
        Err(_) => todo!(),
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

fn play(
    mut reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: &DecoderOptions,
) -> Result<()> {
    // If the user provided a track number, select that track if it exists, otherwise, select the
    // first track with a known codec.
    let track = track_num
        .and_then(|t| reader.tracks().get(t))
        .or_else(|| first_supported_track(reader.tracks()));

    let mut track_id = match track {
        Some(track) => track.id,
        _ => return Ok(()),
    };

    // If there is a seek time, seek the reader to the time specified and get the timestamp of the
    // seeked position. All packets with a timestamp < the seeked position will not be played.
    //
    // Note: This is a half-baked approach to seeking! After seeking the reader, packets should be
    // decoded and *samples* discarded up-to the exact *sample* indicated by required_ts. The
    // current approach will discard excess samples if seeking to a sample within a packet.
    let seek_ts = if let Some(time) = seek_time {
        let seek_to = SeekTo::Time {
            time: Time::from(time),
            track_id: Some(track_id),
        };

        // Attempt the seek. If the seek fails, ignore the error and return a seek timestamp of 0 so
        // that no samples are trimmed.
        match reader.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => seeked_to.required_ts,
            Err(Error::ResetRequired) => {
                track_id = first_supported_track(reader.tracks()).unwrap().id;
                0
            }
            Err(err) => {
                // Don't give-up on a seek error.
                warn!("seek error: {}", err);
                0
            }
        }
    } else {
        // If not seeking, the seek timestamp is 0.
        0
    };

    // The audio output device.
    let mut audio_output = None;

    let mut track_info = PlayTrackOptions { track_id, seek_ts };

    let result = loop {
        match play_track(&mut reader, &mut audio_output, track_info, decode_opts) {
            Err(Error::ResetRequired) => {
                // The demuxer indicated that a reset is required. This is sometimes seen with
                // streaming OGG (e.g., Icecast) wherein the entire contents of the container change
                // (new tracks, codecs, metadata, etc.). Therefore, we must select a new track and
                // recreate the decoder.

                // Select the first supported track since the user's selected track number might no
                // longer be valid or make sense.
                let track_id = first_supported_track(reader.tracks()).unwrap().id;
                track_info = PlayTrackOptions {
                    track_id,
                    seek_ts: 0,
                };
            }
            res => break res,
        }
    };

    // Flush the audio output to finish playing back any leftover samples.
    if let Some(audio_output) = audio_output.as_mut() {
        audio_output.flush()
    }

    result
}

fn play_track(
    reader: &mut Box<dyn FormatReader>,
    audio_output: &mut Option<Box<dyn AudioOutput>>,
    play_opts: PlayTrackOptions,
    decode_opts: &DecoderOptions,
) -> Result<()> {
    // Get the selected track using the track ID.
    let track = match reader
        .tracks()
        .iter()
        .find(|track| track.id == play_opts.track_id)
    {
        Some(track) => track,
        _ => return Ok(()),
    };

    // Create a decoder for the track.
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, decode_opts)?;

    // Decode and play the packets belonging to the selected track.
    let result = loop {
        // Get the next packet from the format reader.
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break Ok(())
            }
            Err(err) => break Err(err),
        };

        // If the packet does not belong to the selected track, skip it.
        if packet.track_id() != play_opts.track_id {
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
                if packet.ts() >= play_opts.seek_ts {
                    if let Some(audio_output) = audio_output {
                        audio_output.write(decoded).unwrap()
                    }
                }
            }
            Err(Error::DecodeError(err)) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                warn!("decode error: {}", err);
            }
            Err(err) => break Err(err),
        }
    };
    debug!("Play finished with result {:?}", result);

    // Regardless of result, finalize the decoder to get the verification result.
    let finalize_result = decoder.finalize();

    if let Some(verify_ok) = finalize_result.verify_ok {
        if verify_ok {
            info!("verification passed");
        } else {
            info!("verification failed");
        }
    }

    result
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
