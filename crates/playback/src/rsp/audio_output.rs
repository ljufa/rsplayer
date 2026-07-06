//! [`AudioOutput`] — the one output path for every platform and every
//! stream (local playback and multiroom sink alike).
//!
//! Despite the crate's Linux heritage this is pure cpal: ALSA/PipeWire,
//! `CoreAudio`, WASAPI and ASIO all go through here. The writer thread pushes
//! decoded buffers into a lock-free SPSC ring (sized `ring_buffer_size_ms`,
//! blocking when full — that back-pressure paces the decode loop); the cpal
//! callback drains it, applying software volume last so volume changes act
//! within one device buffer. Between push and drain sit the format-typed
//! writers: rubato FFT resampling when the device can't do the source rate,
//! channel mapping, EQ (`DspHandle` pending-swap) and VU metering.
//!
//! Opening negotiates the sample format/rate/channels against the device's
//! capabilities with retry ladders for drivers that reject configs they
//! advertise; DSD sources take a separate branch (native DSD formats, no
//! PCM processing). For multiroom this file also hosts the
//! playback-position sensor (`playback_lag_micros`: ring backlog + filtered,
//! then frozen, driver-reported latency) and `prefill_silence_ms` (leader
//! self-delay through the normal writer path).

use anyhow::{Error, Result};
use api_models::settings::RsPlayerSettings;
use log::info;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};
use std::sync::Arc;
use symphonia::core::audio::{AudioSpec, GenericAudioBufferRef};

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use crate::rsp::tee::MonoClock;

/// Number of consecutive error callbacks before the stream is considered
/// fatally broken.  Transient ALSA errors (xruns, timestamp glitches) on
/// resource-constrained hardware like `RPi` Zero are common and recoverable.
/// Without any sleep in the error callback the counter can increment very
/// rapidly, so keep this high enough to absorb bursts while still detecting
/// genuine hardware failures.
const ERROR_THRESHOLD: u32 = 30;

/// Stop updating the device-latency estimate after this many callbacks
/// (~6s at the default 4096-frame buffer): PipeWire/ALSA report values that
/// ramp while their buffers fill, and multiroom sync needs a stable
/// reference more than a live one.
const LATENCY_FREEZE_AFTER_CALLBACKS: u32 = 64;

use crate::rsp::device_capabilities::{fallback_rate_candidates, find_device_channels, find_device_rate};
use crate::rsp::dsd::DsdU32;
use crate::rsp::vumeter::{VUMeter, cubic_gain};
use dsp::DspHandle;
use dsp::Equalizer;

/// Push all of `data` into the ring buffer, blocking while it is full.
///
/// A 1s timeout means the consumer (cpal callback) has stalled without
/// necessarily reporting errors — treat it as a write failure instead of
/// silently dropping the samples.
fn push_to_ring<T: Clone + Copy + Default>(producer: &rb::Producer<T>, data: &[T], error_count: &AtomicU32) -> Result<()> {
    let mut remaining = data;
    loop {
        if error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
            return Err(Error::msg("Audio output error detected during write"));
        }
        match producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            Ok(Some(written)) => remaining = &remaining[written..],
            Ok(None) => return Ok(()),
            Err(e) => return Err(Error::msg(format!("Audio ring buffer write stalled: {e:?}"))),
        }
    }
}

use symphonia::core::audio::conv::{ConvertibleSample, FromSample, IntoSample};
use symphonia::core::audio::sample::Sample;

use cpal::traits::{DeviceTrait, StreamTrait};
use rb::{RB, RbConsumer, RbInspector, RbProducer, SpscRb};

use log::{debug, error, warn};

trait AudioWriter: Send {
    fn write(
        &mut self,
        decoded: GenericAudioBufferRef<'_>,
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
    samples: Vec<T>,
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
        decoded: GenericAudioBufferRef<'_>,
        dsp: &mut Option<DspState>,
        vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        let needs_channel_map = self.source_channels != self.output_channels;
        let samples_needed = decoded.frames() * self.source_channels;
        self.samples.clear();
        self.samples.resize(samples_needed, T::MID);
        decoded.copy_to_slice_interleaved(&mut self.samples);

        if needs_channel_map {
            // Map channels (e.g. mono → stereo) into reusable buffer.
            self.channel_buf.clear();
            for frame in self.samples.chunks(self.source_channels) {
                for ch in 0..self.output_channels {
                    self.channel_buf.push(frame[ch.min(self.source_channels - 1)]);
                }
            }
        }

        // Equalizer — skipped entirely when no filters are configured.
        if let Some(dsp) = dsp
            && dsp.handle.has_filters.load(Ordering::Acquire)
        {
            if needs_channel_map {
                dsp.equalizer.process_samples(&mut self.channel_buf);
            } else {
                dsp.equalizer.process_samples(&mut self.samples);
            }
        }

        // VU metering — reads pre-ring-buffer samples; the meter itself
        // applies the software gain factor when software volume is active.
        if let Some(vu) = vu_meter {
            if needs_channel_map {
                vu.update_peaks(self.output_channels, &self.channel_buf);
            } else {
                vu.update_peaks(self.output_channels, &self.samples);
            }
        }

        // Push to ring buffer. Software gain is applied post-ring-buffer
        // in the cpal output callback so volume changes take effect within
        // the cpal buffer latency rather than the ring_buffer_size_ms latency.
        let remaining: &[T] = if needs_channel_map { &self.channel_buf } else { &self.samples };
        push_to_ring(&self.producer, remaining, error_count)
    }
}

struct DsdWriter {
    producer: rb::Producer<DsdU32>,
    samples: Vec<DsdU32>,
}

impl AudioWriter for DsdWriter {
    fn write(
        &mut self,
        decoded: GenericAudioBufferRef<'_>,
        _dsp: &mut Option<DspState>,
        _vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        // DSD — copy straight to ring buffer, no DSP or VU conversion.
        let samples_needed = decoded.frames() * decoded.spec().channels().count();
        self.samples.clear();
        self.samples.resize(samples_needed, DsdU32::MID);
        decoded.copy_to_slice_interleaved(&mut self.samples);
        push_to_ring(&self.producer, &self.samples, error_count)
    }
}

struct ResamplingPcmWriter<T>
where
    T: Sample,
{
    producer: rb::Producer<T>,
    samples: Vec<f32>,
    resampler: Fft<f32>,
    channels: usize,
    output_channels: usize,
    channel_in: Vec<Vec<f32>>,
    channel_out: Vec<Vec<f32>>,
    /// Interleaved f32 staging buffer — EQ and VU run here, before the
    /// final conversion to the device sample type, to avoid double
    /// quantization on integer formats.
    interleaved_f32: Vec<f32>,
    interleaved_out: Vec<T>,
}

impl<T> AudioWriter for ResamplingPcmWriter<T>
where
    T: cpal::Sample + FromSample<f32> + IntoSample<f32> + Send + 'static + ConvertibleSample,
{
    fn write(
        &mut self,
        decoded: GenericAudioBufferRef<'_>,
        dsp: &mut Option<DspState>,
        vu_meter: &mut Option<VUMeter>,
        error_count: &AtomicU32,
    ) -> Result<()> {
        let samples_needed = decoded.frames() * self.channels;
        self.samples.clear();
        self.samples.resize(samples_needed, f32::MID);
        decoded.copy_to_slice_interleaved(&mut self.samples);

        // De-interleave into per-channel buffers.
        for ch in &mut self.channel_in {
            ch.clear();
        }
        for (i, &s) in self.samples.iter().enumerate() {
            self.channel_in[i % self.channels].push(s);
        }

        // Resample.
        let actual_frames = self.channel_in.first().map_or(0, Vec::len);
        let indexing = rubato::Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: Some(actual_frames),
            active_channels_mask: None,
        };
        let input_adapter = SequentialSliceOfVecs::new(&self.channel_in, self.channels, actual_frames)
            .map_err(|e| Error::msg(format!("resampler input buffer error: {e}")))?;
        let out_max = self.resampler.output_frames_max();
        let mut output_adapter = SequentialSliceOfVecs::new_mut(&mut self.channel_out, self.channels, out_max)
            .map_err(|e| Error::msg(format!("resampler output buffer error: {e}")))?;
        let (_in_frames, out_frames) = self
            .resampler
            .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
            .map_err(|e| Error::msg(format!("resample error: {e}")))?;

        // Re-interleave at device rate, mapping channels if the device
        // requires a different count (e.g. mono→stereo). Stay in f32 so the
        // equalizer and VU meter work on full-precision samples.
        self.interleaved_f32.clear();
        for frame in 0..out_frames {
            for ch in 0..self.output_channels {
                let src_ch = ch.min(self.channels - 1);
                self.interleaved_f32.push(self.channel_out[src_ch][frame]);
            }
        }

        // Equalizer at device rate.
        if let Some(dsp) = dsp
            && dsp.handle.has_filters.load(Ordering::Acquire)
        {
            dsp.equalizer.process_samples(&mut self.interleaved_f32);
        }

        // VU metering.
        if let Some(vu) = vu_meter {
            vu.update_peaks(self.output_channels, &self.interleaved_f32);
        }

        // Convert to the device sample type only once, at the very end.
        // Software volume gain is applied later in the cpal output callback.
        self.interleaved_out.clear();
        self.interleaved_out
            .extend(self.interleaved_f32.iter().map(|&s| <T as FromSample<f32>>::from_sample(s)));

        push_to_ring(&self.producer, &self.interleaved_out, error_count)
    }
}

/// DSP-related state bundled together.  `None` in `AudioOutput` when DSP is
/// disabled or the format is DSD (which cannot be processed).
struct DspState {
    /// Playback-thread-exclusive equalizer — never shared, never locked
    /// during processing.  Replaced by swapping in a pending update.
    equalizer: Equalizer,
    /// Shared handle — only the two `Arc`s cross the thread boundary.
    /// No `Mutex<DspProcessor>` is held here.
    handle: DspHandle,
}

pub struct AudioOutput {
    writer: Box<dyn AudioWriter>,
    stream: cpal::Stream,
    error_count: Arc<AtomicU32>,
    /// Returns the number of samples still queued in the ring buffer —
    /// used by `drain` to let the tail of a song play out before pausing.
    ring_fill: Box<dyn Fn() -> usize + Send>,
    /// DSP processing state — `None` when DSP is disabled or format is DSD.
    dsp: Option<DspState>,
    /// VU meter — `None` when VU metering is disabled.
    vu_meter: Option<VUMeter>,
    /// Device buffer latency (µs) reported by the driver at the last output
    /// callback, and the `MonoClock` time it was measured — together with
    /// the ring fill this yields the playback position for multiroom sync.
    device_latency_micros: Arc<AtomicU64>,
    latency_measured_at_micros: Arc<AtomicU64>,
    output_rate: u32,
    output_channels: usize,
}

#[allow(clippy::too_many_arguments)]
impl AudioOutput {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines, deprecated)]
    pub fn new(
        spec: AudioSpec,
        duration: u64,
        device: &cpal::Device,
        rsp_settings: &RsPlayerSettings,
        is_dsd: bool,
        is_asio: bool,
        dsp_handle: Option<&DspHandle>,
        vu_meter: Option<VUMeter>,
        software_gain: Option<&Arc<AtomicU8>>,
    ) -> Result<AudioOutput> {
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
            let dsd_config = supported_configs_range.into_iter().find(|c: &cpal::SupportedStreamConfigRange| {
                let is_dsd_fmt = matches!(
                    c.sample_format(),
                    cpal::SampleFormat::DsdU32 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU8
                );
                is_dsd_fmt && c.min_sample_rate() <= spec.rate() && c.max_sample_rate() >= spec.rate()
            });

            if let Some(dsd_c) = dsd_config {
                info!("Using DSD format: {}", dsd_c.sample_format());
                (dsd_c.with_sample_rate(spec.rate()).config(), dsd_c.sample_format())
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
        } else if let Some(fixed) = rsp_settings.fixed_output_sample_rate.filter(|&r| r != spec.rate()) {
            info!(
                "Fixed output sample rate {}Hz configured, will resample from {}Hz",
                fixed,
                spec.rate()
            );
            Some(fixed)
        } else {
            find_device_rate(device, spec.rate())
        };
        if let Some(rate) = device_rate {
            info!("Device does not support {}Hz natively, will resample to {}Hz", spec.rate(), rate);
        }

        let device_channels = if is_dsd {
            None
        } else {
            #[allow(clippy::cast_possible_truncation)]
            {
                find_device_channels(device, spec.channels().count() as u16)
            }
        };
        if let Some(ch) = device_channels {
            info!(
                "Device does not support {} channels, will map to {} channels",
                spec.channels().count(),
                ch
            );
        }

        // Rebuild the equalizer for this track's spec.  Skip for DSD.
        // Use the output rate and output channels so filter coefficients are correct.
        let effective_dsp = if is_dsd {
            None
        } else if let Some(handle) = dsp_handle {
            let dsp_rate = device_rate.unwrap_or_else(|| spec.rate()) as usize;
            let dsp_channels = device_channels.map_or_else(|| spec.channels().count(), |ch| ch as usize);
            handle.rebuild(dsp_channels, dsp_rate);
            Some(handle.clone())
        } else {
            None
        };

        // For DSD streams, VU metering is also skipped.
        let effective_vu = if is_dsd { None } else { vu_meter };
        let spec_clone = spec;

        // Determine which ALSA buffer size(s) to try.
        //
        // When the user has not configured a specific size, prefer Fixed(4096)
        // over the driver default.  Many async USB audio devices (e.g. Amanero
        // Combo 768) produce `alsa::poll() POLLERR` with the driver's default
        // period (often 256 frames = 5.8 ms at 44.1 kHz) because the RPi USB
        // controller cannot sustain that interrupt rate.  Fixed(4096) ≈ 90 ms
        // is well within the USB controller's capability.
        //
        // If Fixed(4096) fails to open the stream (rare), fall back to Default.
        let buf_sizes: &[cpal::BufferSize] = if is_dsd || rsp_settings.alsa_buffer_size.is_some() {
            &[] // handled below via explicit_buf
        } else if is_asio {
            // ASIO drivers dictate their own period; Fixed(4096) is rejected.
            // Use the driver-preferred size.
            &[cpal::BufferSize::Default]
        } else {
            &[cpal::BufferSize::Fixed(4096), cpal::BufferSize::Default]
        };
        let explicit_buf = rsp_settings
            .alsa_buffer_size
            .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed);

        let mut result = if buf_sizes.is_empty() {
            AudioOutput::open_with_format(
                &spec_clone,
                duration,
                device,
                rsp_settings,
                effective_dsp.clone(),
                effective_vu.clone(),
                sample_format,
                device_rate,
                device_channels,
                explicit_buf,
                software_gain,
            )
        } else {
            let mut r = Err(Error::msg("no buffer size tried"));
            for buf in buf_sizes {
                r = AudioOutput::open_with_format(
                    &spec_clone,
                    duration,
                    device,
                    rsp_settings,
                    effective_dsp.clone(),
                    effective_vu.clone(),
                    sample_format,
                    device_rate,
                    device_channels,
                    *buf,
                    software_gain,
                );
                if r.is_ok() {
                    break;
                }
                warn!("ALSA buffer size {buf:?} rejected, trying next");
            }
            r
        };

        // Some ALSA drivers (e.g. Merus MA12070P) report a continuous rate
        // range like [44100, 192000] but only accept specific discrete rates.
        // When the stream open fails and we assumed native rate support
        // (device_rate == None), probe candidate rates until one succeeds.
        if result.is_err() && device_rate.is_none() && !is_dsd {
            'rate_fallback: for fallback_rate in fallback_rate_candidates(device, spec_clone.rate()) {
                warn!(
                    "{}Hz rejected by device, trying resampling to {}Hz",
                    spec_clone.rate(),
                    fallback_rate
                );
                if let Some(handle) = dsp_handle {
                    let dsp_channels = device_channels.map_or_else(|| spec_clone.channels().count(), |ch| ch as usize);
                    handle.rebuild(dsp_channels, fallback_rate as usize);
                }
                let buf_sizes_for_rate: &[cpal::BufferSize] = if is_asio {
                    &[cpal::BufferSize::Default]
                } else if rsp_settings.alsa_buffer_size.is_none() && !is_dsd {
                    &[cpal::BufferSize::Fixed(4096), cpal::BufferSize::Default]
                } else {
                    &[]
                };
                if buf_sizes_for_rate.is_empty() {
                    let retry = AudioOutput::open_with_format(
                        &spec_clone,
                        duration,
                        device,
                        rsp_settings,
                        effective_dsp.clone(),
                        effective_vu.clone(),
                        sample_format,
                        Some(fallback_rate),
                        device_channels,
                        explicit_buf,
                        software_gain,
                    );
                    if retry.is_ok() {
                        result = retry;
                        break 'rate_fallback;
                    }
                } else {
                    for buf in buf_sizes_for_rate {
                        let retry = AudioOutput::open_with_format(
                            &spec_clone,
                            duration,
                            device,
                            rsp_settings,
                            effective_dsp.clone(),
                            effective_vu.clone(),
                            sample_format,
                            Some(fallback_rate),
                            device_channels,
                            *buf,
                            software_gain,
                        );
                        if retry.is_ok() {
                            result = retry;
                            break 'rate_fallback;
                        }
                    }
                }
            }
        }

        result
    }

    #[allow(clippy::too_many_lines)]
    fn open_with_format(
        spec: &AudioSpec,
        duration: u64,
        device: &cpal::Device,
        rsp_settings: &RsPlayerSettings,
        dsp_handle: Option<DspHandle>,
        vu_meter: Option<VUMeter>,
        sample_format: cpal::SampleFormat,
        device_rate: Option<u32>,
        device_channels: Option<u16>,
        buffer_size: cpal::BufferSize,
        software_gain: Option<&Arc<AtomicU8>>,
    ) -> Result<AudioOutput> {
        let source_channels = spec.channels().count();
        let output_channels = device_channels.map_or(source_channels, |ch| ch as usize);
        let output_rate = device_rate.unwrap_or_else(|| spec.rate());

        #[allow(clippy::cast_possible_truncation)]
        let config = cpal::StreamConfig {
            channels: output_channels as cpal::ChannelCount,
            sample_rate: output_rate,
            buffer_size,
        };

        let ring_len = ((rsp_settings.ring_buffer_size_ms * output_rate as usize) / 1000) * output_channels;
        let error_count = Arc::new(AtomicU32::new(0));
        let error_count_clone = error_count.clone();
        let device_latency_micros = Arc::new(AtomicU64::new(0));
        let latency_measured_at_micros = Arc::new(AtomicU64::new(0));

        // Build the stream and format-specific writer in one match.
        // Each arm creates its own typed ring buffer and sample buffer.
        macro_rules! build_pcm_variant {
            ($T:ty) => {{
                let ring_buf = SpscRb::<$T>::new(ring_len);
                let (producer, consumer) = (ring_buf.producer(), ring_buf.consumer());
                let ec_data = error_count_clone.clone();
                let gain_level = software_gain.cloned();
                let cb_latency = device_latency_micros.clone();
                let cb_latency_at = latency_measured_at_micros.clone();
                let mut cb_samples_taken: u32 = 0;
                let stream = device
                    .build_output_stream(
                        config,
                        move |data: &mut [$T], info: &cpal::OutputCallbackInfo| {
                            // Driver-reported time until this buffer reaches
                            // the DAC — the multiroom playback-position sensor.
                            // Reports are jumpy on some drivers (HDA Intel
                            // PCH) and ramp up while server-side buffers fill
                            // (PipeWire), so: low-pass filter (EWMA, α=1/8),
                            // then freeze the value once warmed up — after
                            // that only the ring backlog tracks drift.
                            let ts = info.timestamp();
                            let latency = ts.playback.duration_since(ts.callback);
                            if !latency.is_zero() {
                                if cb_samples_taken < LATENCY_FREEZE_AFTER_CALLBACKS {
                                    cb_samples_taken += 1;
                                    let sample = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
                                    let prev = cb_latency.load(Ordering::Relaxed);
                                    let filtered = if prev == 0 { sample } else { (prev * 7 + sample) / 8 };
                                    cb_latency.store(filtered, Ordering::Relaxed);
                                }
                                // Keep the measurement fresh so the aging
                                // model in playback_lag_micros stays valid.
                                cb_latency_at.store(MonoClock::now_micros(), Ordering::Relaxed);
                            }
                            let written = consumer.read(data).unwrap_or(0);
                            data[written..]
                                .iter_mut()
                                .for_each(|s| *s = <$T as cpal::Sample>::EQUILIBRIUM);
                            // Apply software volume gain after draining the ring buffer
                            // so volume changes take effect within the cpal buffer
                            // latency, not the ring_buffer_size_ms latency.
                            if let Some(ref level) = gain_level {
                                let vol = level.load(Ordering::Relaxed);
                                if vol < 100 {
                                    let g = cubic_gain(vol);
                                    for s in &mut data[..written] {
                                        let f: f32 = (*s).into_sample();
                                        *s = (f * g).into_sample();
                                    }
                                }
                            }
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
                            }
                        },
                        None,
                    )
                    .map_err(|e| {
                        error!("audio output stream open error: {e}");
                        Error::from(e)
                    })?;
                let writer: Box<dyn AudioWriter> = if let Some(dev_rate) = device_rate {
                    let resampler = Fft::<f32>::new(
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            spec.rate() as usize
                        },
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            dev_rate as usize
                        },
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            duration as usize
                        },
                        2,
                        source_channels,
                        FixedSync::Input,
                    )
                    .map_err(|e| Error::msg(format!("failed to create resampler: {e}")))?;
                    let channel_in = vec![vec![0.0f32; resampler.input_frames_max()]; source_channels];
                    let mut channel_out = vec![vec![0.0f32; resampler.output_frames_max()]; source_channels];
                    // Ensure output buffers have capacity for max output frames.
                    for ch in &mut channel_out {
                        ch.resize(resampler.output_frames_max(), 0.0);
                    }
                    let max_out_samples = resampler.output_frames_max() * output_channels;
                    Box::new(ResamplingPcmWriter {
                        producer,
                        samples: Vec::with_capacity(usize::try_from(duration).unwrap_or(0) * source_channels),
                        resampler,
                        channels: source_channels,
                        output_channels,
                        channel_in,
                        channel_out,
                        interleaved_f32: Vec::with_capacity(max_out_samples),
                        interleaved_out: Vec::with_capacity(max_out_samples),
                    })
                } else {
                    Box::new(PcmWriter {
                        producer,
                        samples: Vec::with_capacity(usize::try_from(duration).unwrap_or(0) * source_channels),
                        source_channels,
                        output_channels,
                        channel_buf: Vec::new(),
                    })
                };
                let ring_fill: Box<dyn Fn() -> usize + Send> = Box::new(move || ring_buf.count());
                (stream, writer, ring_fill)
            }};
        }

        let (stream, writer, ring_fill) = match sample_format {
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
                        config,
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
                    samples: Vec::with_capacity(usize::try_from(duration).unwrap_or(0) * spec.channels().count()),
                });
                let ring_fill: Box<dyn Fn() -> usize + Send> = Box::new(move || ring_buf.count());
                (stream, writer as Box<dyn AudioWriter>, ring_fill)
            }
            _ => return Err(Error::msg(format!("Unsupported sample format: {sample_format:?}"))),
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

        Ok(AudioOutput {
            writer,
            stream,
            error_count,
            ring_fill,
            dsp,
            vu_meter,
            device_latency_micros,
            latency_measured_at_micros,
            output_rate,
            output_channels,
        })
    }
}

impl AudioOutput {
    /// Time until a sample pushed *now* reaches the DAC: the ring-buffer
    /// backlog plus the device buffer latency reported by the driver at the
    /// last callback (aged, since the device drains between callbacks).
    /// Falls back to the ring backlog alone on drivers without timestamps.
    pub fn playback_lag_micros(&self) -> u64 {
        let ring_frames = (self.ring_fill)() / self.output_channels.max(1);
        let ring_micros = ring_frames as u64 * 1_000_000 / u64::from(self.output_rate.max(1));
        let latency = self.device_latency_micros.load(Ordering::Relaxed);
        let measured_at = self.latency_measured_at_micros.load(Ordering::Relaxed);
        let age = MonoClock::now_micros().saturating_sub(measured_at);
        ring_micros + latency.saturating_sub(age)
    }

    /// True once the driver has reported at least one playback timestamp.
    pub fn has_latency_measurement(&self) -> bool {
        self.latency_measured_at_micros.load(Ordering::Relaxed) > 0
    }

    /// Prefills the ring buffer with `ms` of silence so the first real
    /// sample written afterwards reaches the device roughly `ms` from now.
    /// Used by multiroom playback to delay the leader's own output to the
    /// group's shared start time. `chunk_frames` must not exceed the
    /// `duration` this output was opened with (resampler chunk limit).
    pub fn prefill_silence_ms(&mut self, ms: u32, spec: &AudioSpec, chunk_frames: usize) -> Result<()> {
        let total_frames = usize::try_from(u64::from(spec.rate()) * u64::from(ms) / 1000).unwrap_or(0);
        let chunk = chunk_frames.max(1);
        let mut silence = symphonia::core::audio::AudioBuffer::<f32>::new(spec.clone(), chunk);
        let mut remaining = total_frames;
        while remaining > 0 {
            let n = remaining.min(chunk);
            silence.resize_with_silence(n);
            self.write(GenericAudioBufferRef::F32(&silence))?;
            remaining -= n;
        }
        Ok(())
    }

    pub fn write(&mut self, decoded: GenericAudioBufferRef<'_>) -> Result<()> {
        if self.error_count.load(Ordering::Relaxed) >= ERROR_THRESHOLD {
            return Err(Error::msg("Audio output error detected"));
        }
        if decoded.frames() == 0 {
            return Ok(());
        }

        // Swap in a pending equalizer if one is available.  try_lock avoids
        // blocking; if the writer is mid-update we pick it up next write().
        if let Some(dsp) = &mut self.dsp
            && let Ok(mut slot) = dsp.handle.pending.try_lock()
            && let Some(new_eq) = slot.take()
        {
            info!("Swapped in new equalizer with filters: {}", new_eq.has_filters());
            dsp.equalizer = new_eq;
        }

        // Delegate writing to the format-specific writer.
        self.writer.write(decoded, &mut self.dsp, &mut self.vu_meter, &self.error_count)?;

        if let Some(vu) = &mut self.vu_meter {
            vu.maybe_send_event();
        }

        Ok(())
    }

    /// Stop the stream immediately, discarding anything still queued in the
    /// ring buffer. Use `drain` instead when the song finished naturally.
    pub fn flush(&self) {
        _ = self.stream.pause();
    }

    /// Let the ring buffer play out before pausing the stream, so the tail
    /// of a song (up to `ring_buffer_size_ms`) is not cut off. Aborts early
    /// when a stop is requested, the output errors out, or a deadline based
    /// on the maximum configurable buffer size passes (stalled consumer).
    pub fn drain(&self, stop_signal: &AtomicBool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while (self.ring_fill)() > 0
            && !stop_signal.load(Ordering::Relaxed)
            && self.error_count.load(Ordering::Relaxed) < ERROR_THRESHOLD
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        _ = self.stream.pause();
    }
}
