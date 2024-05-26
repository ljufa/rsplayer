// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Platform-dependant Audio Outputs

use anyhow::Result;
use api_models::settings::RsPlayerSettings;
use log::info;
use symphonia::core::audio::{AudioBufferRef, SignalSpec};

pub trait AudioOutput {
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<()>;
    fn flush(&mut self);

}

mod cpal {

    use std::time::Duration;

    use super::AudioOutput;

    use anyhow::{Error, Result};
    use api_models::settings::RsPlayerSettings;
    use symphonia::core::audio::{AudioBufferRef, RawSample, SampleBuffer, SignalSpec};
    use symphonia::core::conv::ConvertibleSample;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use rb::{RbConsumer, RbProducer, SpscRb, RB};

    use log::{debug, error};

    pub struct CpalAudioOutput;

    trait AudioOutputSample:
        cpal::Sample + cpal::SizedSample + ConvertibleSample + RawSample + std::marker::Send + 'static
    {
    }

    impl AudioOutputSample for f32 {}
    impl AudioOutputSample for i16 {}
    impl AudioOutputSample for u16 {}
    impl AudioOutputSample for u32 {}
    impl AudioOutputSample for i32 {}

    impl CpalAudioOutput {
        pub fn try_open(
            spec: SignalSpec,
            duration: u64,
            audio_device: &str,
            rsp_settings: &RsPlayerSettings,
        ) -> Result<Box<dyn AudioOutput>> {
            // Get default host.
            let host = cpal::default_host();
            let device = host
                .devices()?
                .find(|d| d.name().unwrap_or_default() == audio_device)
                .ok_or_else(|| Error::msg(format!("Device {audio_device} not found!")))?;
            debug!("Spec: {:?}", spec);

            let config = match device.default_output_config() {
                Ok(config) => config,
                Err(err) => {
                    error!("failed to get default output device config: {}", err);
                    return Err(err.into());
                }
            };

            // Select proper playback routine based on sample format.
            match config.sample_format() {
                cpal::SampleFormat::F32 => CpalAudioOutputImpl::<f32>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::I32 => CpalAudioOutputImpl::<i32>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::I16 => CpalAudioOutputImpl::<i16>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::U16 => CpalAudioOutputImpl::<u16>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::U32 => CpalAudioOutputImpl::<u32>::try_open(spec, duration, &device, rsp_settings),
                _ => panic!("Unsupported sample format!"),
            }
        }
    }

    struct CpalAudioOutputImpl<T>
    where
        T: AudioOutputSample,
    {
        ring_buf_producer: rb::Producer<T>,
        sample_buf: SampleBuffer<T>,
        stream: cpal::Stream,
    }

    impl<T: AudioOutputSample> CpalAudioOutputImpl<T> {
        pub fn try_open(
            spec: SignalSpec,
            duration: u64,
            device: &cpal::Device,
            rsp_settings: &RsPlayerSettings,
        ) -> Result<Box<dyn AudioOutput>> {
            let num_channels = spec.channels.count();

            // Output audio stream config.
            #[allow(clippy::cast_possible_truncation)]
            let config = cpal::StreamConfig {
                channels: num_channels as cpal::ChannelCount,
                sample_rate: cpal::SampleRate(spec.rate),
                buffer_size: rsp_settings
                    .alsa_buffer_size
                    .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
            };

            // Create a ring buffer with a capacity
            let ring_len = ((rsp_settings.ring_buffer_size_ms * spec.rate as usize) / 1000) * num_channels;

            let ring_buf = SpscRb::new(ring_len);
            let (ring_buf_producer, ring_buf_consumer) = (ring_buf.producer(), ring_buf.consumer());

            let stream_result = device.build_output_stream(
                &config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    // Write out as many samples as possible from the ring buffer to the audio
                    // output.
                    let written = ring_buf_consumer.read(data).unwrap_or(0);
                    // Mute any remaining samples.
                    data[written..].iter_mut().for_each(|s| *s = T::MID);
                },
                move |err| error!("audio output error: {}", err),
                Some(Duration::from_secs(30)),
            );

            if let Err(err) = stream_result {
                error!("audio output stream open error: {}", err);
                return Err(err.into());
            }

            let stream = stream_result?;

            // Start the output stream.
            if let Err(err) = stream.play() {
                error!("audio output stream play error: {}", err);
                return Err(err.into());
            }

            let sample_buf = SampleBuffer::<T>::new(duration, spec);

            Ok(Box::new(CpalAudioOutputImpl {
                ring_buf_producer,
                sample_buf,
                stream,
            }))
        }
    }

    impl<T: AudioOutputSample> AudioOutput for CpalAudioOutputImpl<T> {
        fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<()> {
            // Do nothing if there are no audio frames.
            if decoded.frames() == 0 {
                return Ok(());
            }

            // Audio samples must be interleaved for cpal. Interleave the samples in the audio
            // buffer into the sample buffer.
            self.sample_buf.copy_interleaved_ref(decoded);

            // Write all the interleaved samples to the ring buffer.
            let mut samples = self.sample_buf.samples();

            while let Ok(Some(written)) = self
                .ring_buf_producer
                .write_blocking_timeout(samples, Duration::from_secs(1))
            {
                samples = &samples[written..];
            }

            Ok(())
        }

        fn flush(&mut self) {
            // Flush is best-effort, ignore the returned result.
            _ = self.stream.pause();
        }

    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: u64,
    audio_device: &str,
    rsp_settings: &RsPlayerSettings,
) -> Result<Box<dyn AudioOutput>> {
    let result = cpal::CpalAudioOutput::try_open(spec, duration, audio_device, rsp_settings);
    if result.is_err() && audio_device.starts_with("hw:") {
        info!(
            "Failed to open audio output {}. Trying with plughw: prefix.",
            audio_device
        );
        return cpal::CpalAudioOutput::try_open(spec, duration, &audio_device.replace("hw:", "plughw:"), rsp_settings);
    }
    result
}
