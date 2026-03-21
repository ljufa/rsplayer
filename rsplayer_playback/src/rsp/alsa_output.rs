use anyhow::{Error, Result};
use api_models::settings::RsPlayerSettings;
use log::info;
use rubato::Resampler;
use std::sync::Arc;
use symphonia::core::audio::{AudioBufferRef, SignalSpec};

use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

/// Number of consecutive error callbacks before the stream is considered
/// fatally broken.  Transient ALSA errors (xruns, timestamp glitches) on
/// resource-constrained hardware like `RPi` Zero are common and recoverable.
const ERROR_THRESHOLD: u32 = 5;

use crate::rsp::dsd::DsdU32;
use crate::rsp::vumeter::VUMeter;
use rsplayer_dsp::DspHandle;
use rsplayer_dsp::Equalizer;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::conv::{ConvertibleSample, FromSample, IntoSample};
use symphonia::core::sample::Sample;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rb::{RbConsumer, RbProducer, SpscRb, RB};

use log::{debug, error};

trait AudioWriter: Send {
    fn write(
        &mut self,
        decoded: AudioBufferRef<'_>,
        dsp: &mut Option<DspState>,
        vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()>;
}

struct PcmWriter<T>
where
    T: Sample,
{
    producer: rb::Producer<T>,
    sample_buf: SampleBuffer<T>,
    source_channels: usize,
    output_channels: usize,
    /// Reused buffer for mono→stereo (or other) channel mapping.
    channel_buf: Vec<T>,
}

impl<T> AudioWriter for PcmWriter<T>
where
    T: cpal::Sample + FromSample<f32> + IntoSample<f32> + Send + 'static + ConvertibleSample,
{
    fn write(
        &mut self,
        decoded: AudioBufferRef<'_>,
        dsp: &mut Option<DspState>,
        vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        self.sample_buf.copy_interleaved_ref(decoded);
        let needs_channel_map = self.source_channels != self.output_channels;

        if needs_channel_map {
            // Map channels (e.g. mono → stereo) into reusable buffer.
            self.channel_buf.clear();
            for frame in self.sample_buf.samples().chunks(self.source_channels) {
                for ch in 0..self.output_channels {
                    self.channel_buf.push(frame[ch.min(self.source_channels - 1)]);
                }
            }
        }

        // Equalizer — skipped entirely when no filters are configured.
        if let Some(ref mut dsp) = dsp {
            if dsp.handle.has_filters.load(Ordering::Acquire) {
                if needs_channel_map {
                    dsp.equalizer.process_samples(&mut self.channel_buf);
                } else {
                    dsp.equalizer.process_samples(self.sample_buf.samples_mut());
                }
            }
        }

        // VU metering.
        if let Some(ref mut vu) = vu_meter {
            if needs_channel_map {
                vu.update_peaks(self.output_channels, &self.channel_buf);
            } else {
                vu.update_peaks(self.output_channels, self.sample_buf.samples());
            }
        }

        // Push to ring buffer.
        let mut remaining: &[T] = if needs_channel_map {
            &self.channel_buf
        } else {
            self.sample_buf.samples()
        };
        while let Ok(Some(written)) = self.producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            remaining = &remaining[written..];
            if error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
                return Err(Error::msg("Audio output error detected during write"));
            }
        }
        Ok(())
    }
}

struct DsdWriter {
    producer: rb::Producer<DsdU32>,
    sample_buf: SampleBuffer<DsdU32>,
}

impl AudioWriter for DsdWriter {
    fn write(
        &mut self,
        decoded: AudioBufferRef<'_>,
        _dsp: &mut Option<DspState>,
        _vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        // DSD — copy straight to ring buffer, no DSP or VU conversion.
        self.sample_buf.copy_interleaved_ref(decoded);
        let mut remaining = self.sample_buf.samples();
        while let Ok(Some(written)) = self.producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            remaining = &remaining[written..];
            if error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
                return Err(Error::msg("Audio output error detected during write"));
            }
        }
        Ok(())
    }
}

struct ResamplingPcmWriter<T>
where
    T: Sample,
{
    producer: rb::Producer<T>,
    sample_buf: SampleBuffer<f32>,
    resampler: rubato::FftFixedIn<f32>,
    channels: usize,
    output_channels: usize,
    channel_in: Vec<Vec<f32>>,
    channel_out: Vec<Vec<f32>>,
    interleaved_out: Vec<T>,
}

impl<T> AudioWriter for ResamplingPcmWriter<T>
where
    T: cpal::Sample + FromSample<f32> + IntoSample<f32> + Send + 'static + ConvertibleSample,
{
    fn write(
        &mut self,
        decoded: AudioBufferRef<'_>,
        dsp: &mut Option<DspState>,
        vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        self.sample_buf.copy_interleaved_ref(decoded);
        let samples = self.sample_buf.samples();

        // De-interleave into per-channel buffers.
        for ch in &mut self.channel_in {
            ch.clear();
        }
        for (i, &s) in samples.iter().enumerate() {
            self.channel_in[i % self.channels].push(s);
        }

        // Resample.
        let (_in_frames, out_frames) = self
            .resampler
            .process_partial_into_buffer(Some(&self.channel_in), &mut self.channel_out, None)
            .map_err(|e| Error::msg(format!("resample error: {e}")))?;

        // Re-interleave and convert to target sample type, mapping
        // channels if the device requires a different count (e.g. mono→stereo).
        self.interleaved_out.clear();
        for frame in 0..out_frames {
            for ch in 0..self.output_channels {
                let src_ch = ch.min(self.channels - 1);
                let sample_f32 = self.channel_out[src_ch][frame];
                self.interleaved_out
                    .push(<T as FromSample<f32>>::from_sample(sample_f32));
            }
        }

        // Equalizer at device rate.
        if let Some(ref mut dsp) = dsp {
            if dsp.handle.has_filters.load(Ordering::Acquire) {
                dsp.equalizer.process_samples(&mut self.interleaved_out);
            }
        }

        // VU metering.
        if let Some(ref mut vu) = vu_meter {
            vu.update_peaks(self.output_channels, &self.interleaved_out);
        }

        // Push to ring buffer.
        let mut remaining: &[T] = &self.interleaved_out;
        while let Ok(Some(written)) = self.producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            remaining = &remaining[written..];
            if error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
                return Err(Error::msg("Audio output error detected during write"));
            }
        }
        Ok(())
    }
}

/// DSP-related state bundled together.  `None` in `AlsaOutput` when DSP is
/// disabled or the format is DSD (which cannot be processed).
struct DspState {
    /// Playback-thread-exclusive equalizer — never shared, never locked
    /// during processing.  Replaced by swapping in a pending update.
    equalizer: Equalizer,
    /// Shared handle — only the two `Arc`s cross the thread boundary.
    /// No `Mutex<DspProcessor>` is held here.
    handle: DspHandle,
}

pub struct AlsaOutput {
    writer: Box<dyn AudioWriter>,
    stream: cpal::Stream,
    error_count: Arc<AtomicU32>,
    /// DSP processing state — `None` when DSP is disabled or format is DSD.
    dsp: Option<DspState>,
    /// VU meter — `None` when VU metering is disabled.
    vu_meter: Option<VUMeter>,
}

#[allow(clippy::too_many_arguments)]
impl AlsaOutput {
    #[allow(clippy::too_many_arguments, deprecated)]
    pub fn new(
        spec: SignalSpec,
        duration: u64,
        audio_device: &str,
        rsp_settings: &RsPlayerSettings,
        is_dsd: bool,
        dsp_handle: Option<&DspHandle>,
        vu_meter: Option<VUMeter>,
    ) -> Result<AlsaOutput> {
        let host = cpal::default_host();
        let device = if audio_device == "default" {
            host.default_output_device()
                .ok_or_else(|| Error::msg("Default audio device not found!"))?
        } else {
            host.devices()?
                .find(|d| d.name().unwrap_or_default() == audio_device)
                .ok_or_else(|| Error::msg(format!("Device {audio_device} not found!")))?
        };

        debug!("Spec: {spec:?}");

        if let Ok(default_cfg) = device.default_output_config() {
            debug!("Device default output config: {default_cfg:?}");
        }
        if let Ok(supported) = device.supported_output_configs() {
            for cfg in supported {
                debug!("Device supported config: {cfg:?}");
            }
        }

        let supported_configs_range = device
            .supported_output_configs()
            .map_err(|e| Error::msg(format!("failed to get supported configs: {e}")))?;

        let (_config, sample_format) = if is_dsd {
            let dsd_config = supported_configs_range
                .into_iter()
                .find(|c: &cpal::SupportedStreamConfigRange| {
                    let is_dsd_fmt = matches!(
                        c.sample_format(),
                        cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
                    );
                    is_dsd_fmt && c.min_sample_rate() <= spec.rate && c.max_sample_rate() >= spec.rate
                });

            if let Some(dsd_c) = dsd_config {
                info!("Using DSD format: {}", dsd_c.sample_format());
                (dsd_c.with_sample_rate(spec.rate).config(), dsd_c.sample_format())
            } else {
                info!("DSD requested but DSD format not found, falling back to default.");
                let default = device
                    .default_output_config()
                    .map_err(|e| Error::msg(format!("failed to get default config: {e}")))?;
                (default.config(), default.sample_format())
            }
        } else {
            let default = device
                .default_output_config()
                .map_err(|e| Error::msg(format!("failed to get default config: {e}")))?;
            (default.config(), default.sample_format())
        };

        let device_rate = if is_dsd {
            None
        } else {
            find_device_rate(&device, spec.rate)
        };
        if let Some(rate) = device_rate {
            info!(
                "Device does not support {}Hz natively, will resample to {}Hz",
                spec.rate, rate
            );
        }

        let device_channels = if is_dsd {
            None
        } else {
            find_device_channels(&device, spec.channels.count() as u16)
        };
        if let Some(ch) = device_channels {
            info!(
                "Device does not support {} channels, will map to {} channels",
                spec.channels.count(),
                ch
            );
        }

        // Rebuild the equalizer for this track's spec.  Skip for DSD.
        // Use the output rate and output channels so filter coefficients are correct.
        let effective_dsp = if is_dsd {
            None
        } else if let Some(handle) = dsp_handle {
            let dsp_rate = device_rate.unwrap_or(spec.rate) as usize;
            let dsp_channels = device_channels.map_or(spec.channels.count(), |ch| ch as usize);
            handle.rebuild(dsp_channels, dsp_rate);
            Some(handle.clone())
        } else {
            None
        };

        // For DSD streams, VU metering is also skipped.
        let effective_vu = if is_dsd { None } else { vu_meter };

        AlsaOutput::open_with_format(
            spec,
            duration,
            &device,
            rsp_settings,
            effective_dsp,
            effective_vu,
            sample_format,
            device_rate,
            device_channels,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn open_with_format(
        spec: SignalSpec,
        duration: u64,
        device: &cpal::Device,
        rsp_settings: &RsPlayerSettings,
        dsp_handle: Option<DspHandle>,
        vu_meter: Option<VUMeter>,
        sample_format: cpal::SampleFormat,
        device_rate: Option<u32>,
        device_channels: Option<u16>,
    ) -> Result<AlsaOutput> {
        let source_channels = spec.channels.count();
        let output_channels = device_channels.map_or(source_channels, |ch| ch as usize);
        let output_rate = device_rate.unwrap_or(spec.rate);

        #[allow(clippy::cast_possible_truncation)]
        let config = cpal::StreamConfig {
            channels: output_channels as cpal::ChannelCount,
            sample_rate: output_rate,
            buffer_size: rsp_settings
                .alsa_buffer_size
                .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
        };

        let ring_len = ((rsp_settings.ring_buffer_size_ms * output_rate as usize) / 1000) * output_channels;
        let error_count = Arc::new(AtomicU32::new(0));
        let error_count_clone = error_count.clone();

        // Build the stream and format-specific writer in one match.
        // Each arm creates its own typed ring buffer and sample buffer.
        macro_rules! build_pcm_variant {
            ($T:ty) => {{
                let ring_buf = SpscRb::<$T>::new(ring_len);
                let (producer, consumer) = (ring_buf.producer(), ring_buf.consumer());
                let ec_data = error_count_clone.clone();
                let stream = device
                    .build_output_stream(
                        &config,
                        move |data: &mut [$T], _: &cpal::OutputCallbackInfo| {
                            let written = consumer.read(data).unwrap_or(0);
                            data[written..]
                                .iter_mut()
                                .for_each(|s| *s = <$T as cpal::Sample>::EQUILIBRIUM);
                            // Successful callback — reset transient error counter.
                            ec_data.store(0, Ordering::Relaxed);
                        },
                        {
                            let ec_err = error_count_clone.clone();
                            move |err| {
                                let count = ec_err.fetch_add(1, Ordering::Relaxed) + 1;
                                if count <= ERROR_THRESHOLD {
                                    error!("audio output error ({count}/{ERROR_THRESHOLD}): {err}");
                                }
                                thread::sleep(Duration::from_millis(200));
                            }
                        },
                        None,
                    )
                    .map_err(|e| {
                        error!("audio output stream open error: {e}");
                        Error::from(e)
                    })?;
                let writer: Box<dyn AudioWriter> = if let Some(dev_rate) = device_rate {
                    let resampler = rubato::FftFixedIn::<f32>::new(
                        #[allow(clippy::cast_possible_truncation)]
                        {spec.rate as usize},
                        #[allow(clippy::cast_possible_truncation)]
                        {dev_rate as usize},
                        #[allow(clippy::cast_possible_truncation)]
                        {duration as usize},
                        2,
                        source_channels,
                    )
                    .map_err(|e| Error::msg(format!("failed to create resampler: {e}")))?;
                    let channel_in = resampler.input_buffer_allocate(true);
                    let mut channel_out = resampler.output_buffer_allocate(true);
                    // Ensure output buffers have capacity for max output frames.
                    for ch in &mut channel_out {
                        ch.resize(resampler.output_frames_max(), 0.0);
                    }
                    let max_out_samples = resampler.output_frames_max() * output_channels;
                    Box::new(ResamplingPcmWriter {
                        producer,
                        sample_buf: SampleBuffer::<f32>::new(duration, spec),
                        resampler,
                        channels: source_channels,
                        output_channels,
                        channel_in,
                        channel_out,
                        interleaved_out: Vec::with_capacity(max_out_samples),
                    })
                } else {
                    Box::new(PcmWriter {
                        producer,
                        sample_buf: SampleBuffer::<$T>::new(duration, spec),
                        source_channels,
                        output_channels,
                        channel_buf: Vec::new(),
                    })
                };
                (stream, writer)
            }};
        }

        let (stream, writer) = match sample_format {
            cpal::SampleFormat::F32 => build_pcm_variant!(f32),
            cpal::SampleFormat::I32 => build_pcm_variant!(i32),
            cpal::SampleFormat::I16 => build_pcm_variant!(i16),
            cpal::SampleFormat::U16 => build_pcm_variant!(u16),
            cpal::SampleFormat::U32 => build_pcm_variant!(u32),
            cpal::SampleFormat::DsdU32 => {
                let ring_buf = SpscRb::<DsdU32>::new(ring_len);
                let (producer, consumer) = (ring_buf.producer(), ring_buf.consumer());
                let ec_data = error_count_clone.clone();
                let stream = device
                    .build_output_stream(
                        &config,
                        move |data: &mut [DsdU32], _: &cpal::OutputCallbackInfo| {
                            let written = consumer.read(data).unwrap_or(0);
                            data[written..].iter_mut().for_each(|s| *s = DsdU32::MID);
                            ec_data.store(0, Ordering::Relaxed);
                        },
                        {
                            let ec_err = error_count_clone;
                            move |err| {
                                let count = ec_err.fetch_add(1, Ordering::Relaxed) + 1;
                                if count <= ERROR_THRESHOLD {
                                    error!("audio output error ({count}/{ERROR_THRESHOLD}): {err}");
                                }
                                thread::sleep(Duration::from_millis(200));
                            }
                        },
                        None,
                    )
                    .map_err(|e| {
                        error!("audio output stream open error: {e}");
                        Error::from(e)
                    })?;
                let writer = Box::new(DsdWriter {
                    producer,
                    sample_buf: SampleBuffer::<DsdU32>::new(duration, spec),
                });
                (stream, writer as Box<dyn AudioWriter>)
            }
            _ => panic!("Unsupported sample format: {sample_format:?}"),
        };

        if let Err(err) = stream.play() {
            error!("audio output stream play error: {err}");
            return Err(err.into());
        }

        // Extract the initial pending equalizer from the handle (placed there
        // by DspProcessor::rebuild before the playback thread was spawned).
        let dsp = dsp_handle.map(|handle| {
            let equalizer = handle
                .pending
                .lock()
                .ok()
                .and_then(|mut slot| slot.take())
                .unwrap_or_else(|| Equalizer::new(0));
            DspState { equalizer, handle }
        });

        Ok(AlsaOutput {
            writer,
            stream,
            error_count,
            dsp,
            vu_meter,
        })
    }
}

impl AlsaOutput {
    pub fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<()> {
        if self.error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
            return Err(Error::msg("Audio output error detected"));
        }
        if decoded.frames() == 0 {
            return Ok(());
        }

        // Swap in a pending equalizer if one is available.  try_lock avoids
        // blocking; if the writer is mid-update we pick it up next write().
        if let Some(ref mut dsp) = self.dsp {
            if let Ok(mut slot) = dsp.handle.pending.try_lock() {
                if let Some(new_eq) = slot.take() {
                    info!("Swapped in new equalizer with filters: {}", new_eq.has_filters());
                    dsp.equalizer = new_eq;
                }
            }
        }

        // Delegate writing to the format-specific writer.
        self.writer
            .write(decoded, &mut self.dsp, &mut self.vu_meter, &self.error_count)?;

        if let Some(ref mut vu) = self.vu_meter {
            vu.maybe_send_event();
        }

        Ok(())
    }

    pub fn flush(&self) {
        _ = self.stream.pause();
    }
}

/// Check if the device supports `source_rate` natively.
/// Returns `None` if supported, or `Some(best_rate)` to resample to.
///
/// Prefers integer multiples of the source rate (e.g. 22050→44100 at 2×)
/// for cleaner resampling, falling back to the closest supported rate.
///
/// Set `RSPLAYER_RESAMPLE_TO=<rate>` to force resampling (e.g. for testing).
fn find_device_rate(device: &cpal::Device, source_rate: u32) -> Option<u32> {
    if let Ok(val) = std::env::var("RSPLAYER_RESAMPLE_TO") {
        if let Ok(rate) = val.parse::<u32>() {
            if rate != source_rate {
                info!("RSPLAYER_RESAMPLE_TO={rate} override active");
                return Some(rate);
            }
        }
    }

    let Ok(configs) = device.supported_output_configs() else { return None };

    let mut closest_rate: Option<u32> = None;
    let mut min_distance = u32::MAX;
    // Prefer integer multiples of source_rate (smallest factor first)
    // for better resampling quality (e.g. 22050→44100 at 2× instead of
    // 22050→32000 at ~1.45×).
    let mut best_multiple: Option<u32> = None;
    let mut best_factor = u32::MAX;

    for config in configs {
        if matches!(
            config.sample_format(),
            cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
        ) {
            continue;
        }

        let min_rate = config.min_sample_rate();
        let max_rate = config.max_sample_rate();

        if min_rate <= source_rate && max_rate >= source_rate {
            return None;
        }

        for &rate in &[min_rate, max_rate] {
            let distance = source_rate.abs_diff(rate);
            if distance < min_distance {
                min_distance = distance;
                closest_rate = Some(rate);
            }
            if rate > source_rate && rate % source_rate == 0 {
                let factor = rate / source_rate;
                if factor < best_factor {
                    best_factor = factor;
                    best_multiple = Some(rate);
                }
            }
        }
    }

    best_multiple.or(closest_rate)
}

/// Check if the device supports `source_channels` natively.
/// Returns `None` if supported, or `Some(closest_channels)` to map to.
fn find_device_channels(device: &cpal::Device, source_channels: u16) -> Option<u16> {
    let Ok(configs) = device.supported_output_configs() else {
        return None;
    };

    let mut supported = false;
    let mut closest: Option<u16> = None;
    let mut min_distance = u16::MAX;

    for config in configs {
        if matches!(
            config.sample_format(),
            cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
        ) {
            continue;
        }

        let ch = config.channels();
        if ch == source_channels {
            supported = true;
            break;
        }

        let distance = source_channels.abs_diff(ch);
        if distance < min_distance {
            min_distance = distance;
            closest = Some(ch);
        }
    }

    if supported {
        None
    } else {
        closest
    }
}
