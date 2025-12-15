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

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::AudioOutput;

    use anyhow::{Error, Result};
    use api_models::settings::RsPlayerSettings;
    use symphonia::core::audio::{AudioBufferRef, RawSample, SampleBuffer, SignalSpec};
    use symphonia::core::conv::ConvertibleSample;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use rb::{RbConsumer, RbProducer, SpscRb, RB};

    use log::{debug, error, info};
    use symphonia::core::sample::SampleFormat as SymphoniaSampleFormat;
    use symphonia::core::conv::FromSample;

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
    impl AudioOutputSample for u8 {}
    impl AudioOutputSample for DsdU32 {}

    // DSD Wrapper types
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Default)]
    pub struct DsdU32(pub u32);

    impl cpal::Sample for DsdU32 {
        type Float = f32;
        type Signed = i32;
        const EQUILIBRIUM: Self = DsdU32(0x69696969);
    }

    impl cpal::SizedSample for DsdU32 {
        const FORMAT: cpal::SampleFormat = cpal::SampleFormat::DsdU32;
    }

    // Symphonia Sample trait implementation
    impl std::ops::Add for DsdU32 {
        type Output = Self;
        fn add(self, rhs: Self) -> Self {
            DsdU32(self.0.wrapping_add(rhs.0))
        }
    }

    impl std::ops::Sub for DsdU32 {
        type Output = Self;
        fn sub(self, rhs: Self) -> Self {
            DsdU32(self.0.wrapping_sub(rhs.0))
        }
    }

    impl symphonia::core::sample::Sample for DsdU32 {
        const FORMAT: SymphoniaSampleFormat = SymphoniaSampleFormat::U32; // Proxy
        const EFF_BITS: u32 = 32;
        const MID: Self = DsdU32(0x69696969);
        
        fn clamped(self) -> Self {
            self
        }
    }

    impl RawSample for DsdU32 {
        type RawType = u32;
        fn into_raw_sample(self) -> Self::RawType {
            self.0
        }
    }

    // Implement FromSample for primitives required by ConvertibleSample
    // For types that are not compatible or where conversion is complex (PCM->DSD), we return silence.
    macro_rules! impl_from_sample_for_dsd_dummy {
        ($($t:ty),*) => {
            $(
                impl FromSample<$t> for DsdU32 {
                    fn from_sample(_s: $t) -> Self {
                        DsdU32(0x69696969)
                    }
                }
            )*
        };
    }

    impl_from_sample_for_dsd_dummy!(i8, i16, i24, u8, u16, u24, f32, f64);
    
    // For u32 and i32, we assume they hold packed DSD data and pass it through.
    impl FromSample<u32> for DsdU32 {
        fn from_sample(s: u32) -> Self {
            DsdU32(s)
        }
    }

    impl FromSample<i32> for DsdU32 {
        fn from_sample(s: i32) -> Self {
            DsdU32(s as u32)
        }
    }

    use symphonia::core::conv::IntoSample;
    use symphonia::core::sample::{i24, u24}; // Need these for the macro

    // ConvertibleSample is automatically implemented because DsdU32 implements Sample and all FromSample variants.
    
    // Explicit IntoSample<f32> for DsdU32 required by AudioOutputSample trait bound
    impl IntoSample<f32> for DsdU32 {
        fn into_sample(self) -> f32 {
            0.0
        }
    }

    // Implement cpal::FromSample for DsdU32 relationships required by cpal::Sample
    // DsdU32::Float is f32. DsdU32::Signed is i32.
    // Required: f32 <-> DsdU32, i32 <-> DsdU32

    impl cpal::FromSample<f32> for DsdU32 {
        fn from_sample_(_s: f32) -> Self {
            DsdU32(0x69696969)
        }
    }

    impl cpal::FromSample<DsdU32> for f32 {
        fn from_sample_(_s: DsdU32) -> Self {
            0.0
        }
    }

    impl cpal::FromSample<i32> for DsdU32 {
        fn from_sample_(_s: i32) -> Self {
            DsdU32(0x69696969)
        }
    }

    impl cpal::FromSample<DsdU32> for i32 {
        fn from_sample_(_s: DsdU32) -> Self {
            0
        }
    }

    impl CpalAudioOutput {
        pub fn try_open(
            spec: SignalSpec,
            duration: u64,
            audio_device: &str,
            rsp_settings: &RsPlayerSettings,
            is_dsd: bool,
        ) -> Result<Box<dyn AudioOutput>> {
            // Get default host.
            let host = cpal::default_host();
            let device = host
                .devices()?
                .find(|d| d.name().unwrap_or_default() == audio_device)
                .ok_or_else(|| Error::msg(format!("Device {audio_device} not found!")))?;
            debug!("Spec: {spec:?}");

            // Attempt to find a supported config that matches the spec
            // If checking defaults fails or we want to search for DSD
            let supported_configs_range = device.supported_output_configs()
                .map_err(|e| Error::msg(format!("failed to get supported configs: {e}")))?;

            // Check if we should prefer DSD.
            // If the track is DSD, we should look for DSD formats.
            
            let (_config, sample_format) = if is_dsd {
                // Since supported_configs is an iterator, we need to collect or clone it if we want to search multiple times.
                // Or just search once.
                // Note: DSD rate in ALSA is typically packed (e.g. 88200 for DSD64).
                // spec.rate from DsdDecoder is already packed (88200).
                let dsd_config = supported_configs_range.into_iter().find(|c: &cpal::SupportedStreamConfigRange| {
                    let is_dsd_fmt = matches!(c.sample_format(), cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8);
                    is_dsd_fmt && c.min_sample_rate() <= spec.rate && c.max_sample_rate() >= spec.rate
                });
                
                if let Some(dsd_c) = dsd_config {
                    info!("Using DSD format: {}", dsd_c.sample_format());
                    (dsd_c.with_sample_rate(spec.rate).config(), dsd_c.sample_format())
                } else {
                    // Fallback to default if DSD not supported/found
                    info!("DSD requested but DSD format not found, falling back to default.");
                    let default = device.default_output_config()
                        .map_err(|e| Error::msg(format!("failed to get default config: {e}")))?;
                    (default.config(), default.sample_format())
                }
            } else {
                 // Non-DSD rate, use default config logic
                 let default = device.default_output_config()
                    .map_err(|e| Error::msg(format!("failed to get default config: {e}")))?;
                 (default.config(), default.sample_format())
            };

            // Select proper playback routine based on sample format.
            match sample_format {
                cpal::SampleFormat::F32 => CpalAudioOutputImpl::<f32>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::I32 => CpalAudioOutputImpl::<i32>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::I16 => CpalAudioOutputImpl::<i16>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::U16 => CpalAudioOutputImpl::<u16>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::U32 => CpalAudioOutputImpl::<u32>::try_open(spec, duration, &device, rsp_settings),
                cpal::SampleFormat::DsdU32 => CpalAudioOutputImpl::<DsdU32>::try_open(spec, duration, &device, rsp_settings),
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
        error_flag: Arc<AtomicBool>,
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
                sample_rate: spec.rate,
                buffer_size: rsp_settings
                    .alsa_buffer_size
                    .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
            };

            // Create a ring buffer with a capacity
            let ring_len = ((rsp_settings.ring_buffer_size_ms * spec.rate as usize) / 1000) * num_channels;

            let ring_buf = SpscRb::new(ring_len);
            let (ring_buf_producer, ring_buf_consumer) = (ring_buf.producer(), ring_buf.consumer());
            let error_flag = Arc::new(AtomicBool::new(false));
            let error_flag_clone = error_flag.clone();

            let stream_result = device.build_output_stream(
                &config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    // Write out as many samples as possible from the ring buffer to the audio
                    // output.
                    let written = ring_buf_consumer.read(data).unwrap_or(0);
                    // Mute any remaining samples.
                    data[written..].iter_mut().for_each(|s| *s = T::MID);
                },
                move |err| {
                    error!("audio output error: {err}");
                    error_flag_clone.store(true, Ordering::Relaxed);
                    thread::sleep(Duration::from_millis(800));
                },
                None,
            );

            if let Err(err) = stream_result {
                error!("audio output stream open error: {err}");
                return Err(err.into());
            }

            let stream = stream_result?;

            // Start the output stream.
            if let Err(err) = stream.play() {
                error!("audio output stream play error: {err}");
                
                return Err(err.into());
            }

            let sample_buf = SampleBuffer::<T>::new(duration, spec);

            Ok(Box::new(CpalAudioOutputImpl {
                ring_buf_producer,
                sample_buf,
                stream,
                error_flag,
            }))
        }
    }

    impl<T: AudioOutputSample> AudioOutput for CpalAudioOutputImpl<T> {
        fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<()> {
            if self.error_flag.load(Ordering::Relaxed) {
                return Err(Error::msg("Audio output error detected"));
            }
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
                if self.error_flag.load(Ordering::Relaxed) {
                    return Err(Error::msg("Audio output error detected during write"));
                }
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
    is_dsd: bool,
) -> Result<Box<dyn AudioOutput>> {
    let result = cpal::CpalAudioOutput::try_open(spec, duration, audio_device, rsp_settings, is_dsd);
    if result.is_err() && audio_device.starts_with("hw:") {
        info!(
            "Failed to open audio output {audio_device}. Trying with plughw: prefix."
        );
        return cpal::CpalAudioOutput::try_open(spec, duration, &audio_device.replace("hw:", "plughw:"), rsp_settings, is_dsd);
    }
    result
}
