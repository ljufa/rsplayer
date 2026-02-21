use anyhow::{Error, Result};
use api_models::settings::RsPlayerSettings;
use api_models::state::StateChangeEvent;
use log::info;
use std::sync::Arc;
use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use tokio::sync::broadcast::Sender;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::Duration;

use crate::rsp::dsd::DsdU32;
use crate::rsp::vumeter::VUMeter;
use rsplayer_dsp::DspProcessor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::conv::{FromSample, IntoSample};
use symphonia::core::sample::Sample;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rb::{RbConsumer, RbProducer, SpscRb, RB};

use log::{debug, error};
use rsplayer_dsp::Equalizer;

// Per-format ring-buffer producer and interleaved sample buffer, held as a
// concrete enum variant so we avoid a generic type parameter on `AlsaOutput`.
enum SampleState {
    F32 {
        producer: rb::Producer<f32>,
        sample_buf: SampleBuffer<f32>,
    },
    I32 {
        producer: rb::Producer<i32>,
        sample_buf: SampleBuffer<i32>,
    },
    I16 {
        producer: rb::Producer<i16>,
        sample_buf: SampleBuffer<i16>,
    },
    U16 {
        producer: rb::Producer<u16>,
        sample_buf: SampleBuffer<u16>,
    },
    U32 {
        producer: rb::Producer<u32>,
        sample_buf: SampleBuffer<u32>,
    },
    DsdU32 {
        producer: rb::Producer<DsdU32>,
        sample_buf: SampleBuffer<DsdU32>,
    },
}

pub struct AlsaOutput {
    sample_state: SampleState,
    stream: cpal::Stream,
    error_flag: Arc<AtomicBool>,
    is_dsd: bool,
    /// Whether DSP processing is globally enabled (settings flag).
    dsp_enabled: bool,
    /// VU meter state and logic.
    vu_meter: VUMeter,
    /// Playback-thread-exclusive equalizer — never shared, never locked
    /// during processing.  Replaced by swapping in a pending update.
    equalizer: Equalizer,
    /// Shared coordination state.  The playback thread only touches
    /// `pending` (via `try_lock`) to swap in a new equalizer; it never
    /// holds this lock during DSP processing.
    dsp_state: Arc<std::sync::Mutex<DspProcessor>>,
    /// Lock-free flag — read with `Acquire` ordering at the top of every
    /// `write()` to skip all DSP work when no filters are configured.
    has_filters: Arc<AtomicBool>,
    eq_scratch: Vec<f32>,
}
#[allow(clippy::too_many_arguments)]
impl AlsaOutput {
    pub fn new(
        spec: SignalSpec,
        duration: u64,
        audio_device: &str,
        rsp_settings: &RsPlayerSettings,
        is_dsd: bool,
        changes_tx: Sender<StateChangeEvent>,
        dsp_state: Arc<std::sync::Mutex<DspProcessor>>,
        volume: Arc<AtomicU8>,
    ) -> Result<AlsaOutput> {
        let host = cpal::default_host();
        let device = host
            .devices()?
            .find(|d| d.name().unwrap_or_default() == audio_device)
            .ok_or_else(|| Error::msg(format!("Device {audio_device} not found!")))?;
        debug!("Spec: {spec:?}");

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

        // Rebuild the equalizer for this track's channel count and sample rate.
        // This runs once per track, not per buffer.
        if !is_dsd {
            if let Ok(mut state) = dsp_state.lock() {
                state.rebuild(spec.channels.count(), spec.rate as usize);
            }
        }

        let dsp_enabled = dsp_state
            .lock()
            .map(|state| state.dsp_settings.enabled)
            .unwrap_or(false);
        let vu_meter_enabled = rsp_settings.vu_meter_enabled;

        AlsaOutput::open_with_format(
            spec,
            duration,
            &device,
            rsp_settings,
            is_dsd,
            changes_tx,
            dsp_state,
            volume,
            sample_format,
            dsp_enabled,
            vu_meter_enabled,
        )
    }

    fn open_with_format(
        spec: SignalSpec,
        duration: u64,
        device: &cpal::Device,
        rsp_settings: &RsPlayerSettings,
        is_dsd: bool,
        changes_tx: Sender<StateChangeEvent>,
        dsp_state: Arc<std::sync::Mutex<DspProcessor>>,
        volume: Arc<AtomicU8>,
        sample_format: cpal::SampleFormat,
        dsp_enabled: bool,
        vu_meter_enabled: bool,
    ) -> Result<AlsaOutput> {
        let num_channels = spec.channels.count();

        #[allow(clippy::cast_possible_truncation)]
        let config = cpal::StreamConfig {
            channels: num_channels as cpal::ChannelCount,
            sample_rate: spec.rate,
            buffer_size: rsp_settings
                .alsa_buffer_size
                .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
        };

        let ring_len = ((rsp_settings.ring_buffer_size_ms * spec.rate as usize) / 1000) * num_channels;
        let error_flag = Arc::new(AtomicBool::new(false));
        let error_flag_clone = error_flag.clone();

        // Build the stream and format-specific SampleState in one match.
        // Each arm creates its own typed ring buffer and sample buffer.
        macro_rules! build_pcm_variant {
            ($T:ty, $variant:ident) => {{
                let ring_buf = SpscRb::<$T>::new(ring_len);
                let (producer, consumer) = (ring_buf.producer(), ring_buf.consumer());
                let stream = device
                    .build_output_stream(
                        &config,
                        move |data: &mut [$T], _: &cpal::OutputCallbackInfo| {
                            let written = consumer.read(data).unwrap_or(0);
                            data[written..]
                                .iter_mut()
                                .for_each(|s| *s = <$T as cpal::Sample>::EQUILIBRIUM);
                        },
                        {
                            let ef = error_flag_clone.clone();
                            move |err| {
                                error!("audio output error: {err}");
                                ef.store(true, Ordering::Relaxed);
                                thread::sleep(Duration::from_millis(800));
                            }
                        },
                        None,
                    )
                    .map_err(|e| {
                        error!("audio output stream open error: {e}");
                        Error::from(e)
                    })?;
                let sample_state = SampleState::$variant {
                    producer,
                    sample_buf: SampleBuffer::<$T>::new(duration, spec),
                };
                (stream, sample_state)
            }};
        }

        let (stream, sample_state) = match sample_format {
            cpal::SampleFormat::F32 => build_pcm_variant!(f32, F32),
            cpal::SampleFormat::I32 => build_pcm_variant!(i32, I32),
            cpal::SampleFormat::I16 => build_pcm_variant!(i16, I16),
            cpal::SampleFormat::U16 => build_pcm_variant!(u16, U16),
            cpal::SampleFormat::U32 => build_pcm_variant!(u32, U32),
            cpal::SampleFormat::DsdU32 => {
                let ring_buf = SpscRb::<DsdU32>::new(ring_len);
                let (producer, consumer) = (ring_buf.producer(), ring_buf.consumer());
                let stream = device
                    .build_output_stream(
                        &config,
                        move |data: &mut [DsdU32], _: &cpal::OutputCallbackInfo| {
                            let written = consumer.read(data).unwrap_or(0);
                            data[written..].iter_mut().for_each(|s| *s = DsdU32::MID);
                        },
                        {
                            let ef = error_flag_clone.clone();
                            move |err| {
                                error!("audio output error: {err}");
                                ef.store(true, Ordering::Relaxed);
                                thread::sleep(Duration::from_millis(800));
                            }
                        },
                        None,
                    )
                    .map_err(|e| {
                        error!("audio output stream open error: {e}");
                        Error::from(e)
                    })?;
                let sample_state = SampleState::DsdU32 {
                    producer,
                    sample_buf: SampleBuffer::<DsdU32>::new(duration, spec),
                };
                (stream, sample_state)
            }
            _ => panic!("Unsupported sample format: {sample_format:?}"),
        };

        if let Err(err) = stream.play() {
            error!("audio output stream play error: {err}");
            return Err(err.into());
        }

        let (equalizer, has_filters) = dsp_state
            .lock()
            .map(|s| {
                let eq = s
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take())
                    .unwrap_or_else(|| Equalizer::new(0));
                (eq, s.has_filters.clone())
            })
            .unwrap_or_else(|_| (Equalizer::new(0), Arc::new(AtomicBool::new(false))));

        Ok(AlsaOutput {
            sample_state,
            stream,
            error_flag,
            is_dsd,
            dsp_enabled,
            vu_meter: VUMeter::new(vu_meter_enabled, volume, changes_tx),
            equalizer,
            dsp_state,
            has_filters,
            eq_scratch: Vec::new(),
        })
    }
}

impl AlsaOutput {
    pub fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<()> {
        if self.error_flag.load(Ordering::Relaxed) {
            return Err(Error::msg("Audio output error detected"));
        }
        if decoded.frames() == 0 {
            return Ok(());
        }

        let channels = decoded.spec().channels.count();

        // Swap in a pending equalizer if one is available.  try_lock avoids
        // blocking; if the writer is mid-update we pick it up next write().
        if !self.is_dsd && self.dsp_enabled {
            if let Ok(state) = self.dsp_state.try_lock() {
                if let Ok(mut slot) = state.pending.try_lock() {
                    if let Some(new_eq) = slot.take() {
                        self.equalizer = new_eq;
                    }
                }
            }
        }

        // Interleave decoded samples into the format-specific SampleBuffer,
        // run DSP when active, compute VU peaks, then push to the ring buffer.
        macro_rules! process_pcm {
            ($producer:expr, $sample_buf:expr, $T:ty) => {{
                $sample_buf.copy_interleaved_ref(decoded);

                // Equalizer — skipped entirely when no filters are configured.
                if self.dsp_enabled && self.has_filters.load(Ordering::Acquire) {
                    let samples_mut = $sample_buf.samples_mut();
                    if self.eq_scratch.len() < samples_mut.len() {
                        self.eq_scratch.resize(samples_mut.len(), 0.0);
                    }
                    for (i, s) in samples_mut.iter().enumerate() {
                        self.eq_scratch[i] = (*s).into_sample();
                    }
                    self.equalizer.process(&mut self.eq_scratch[0..samples_mut.len()]);
                    for (i, s) in samples_mut.iter_mut().enumerate() {
                        *s = <$T as FromSample<f32>>::from_sample(self.eq_scratch[i]);
                    }
                }

                // VU metering.
                if self.vu_meter.enabled() {
                    let samples = $sample_buf.samples();
                    self.vu_meter.update_peaks(channels, samples);
                }

                // Push to ring buffer.
                let mut remaining = $sample_buf.samples();
                while let Ok(Some(written)) = $producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
                    remaining = &remaining[written..];
                    if self.error_flag.load(Ordering::Relaxed) {
                        return Err(Error::msg("Audio output error detected during write"));
                    }
                }
            }};
        }

        match &mut self.sample_state {
            SampleState::F32 { producer, sample_buf } => process_pcm!(producer, sample_buf, f32),
            SampleState::I32 { producer, sample_buf } => process_pcm!(producer, sample_buf, i32),
            SampleState::I16 { producer, sample_buf } => process_pcm!(producer, sample_buf, i16),
            SampleState::U16 { producer, sample_buf } => process_pcm!(producer, sample_buf, u16),
            SampleState::U32 { producer, sample_buf } => process_pcm!(producer, sample_buf, u32),
            SampleState::DsdU32 { producer, sample_buf } => {
                // DSD — copy straight to ring buffer, no DSP or VU conversion.
                sample_buf.copy_interleaved_ref(decoded);
                let mut remaining = sample_buf.samples();
                while let Ok(Some(written)) = producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
                    remaining = &remaining[written..];
                    if self.error_flag.load(Ordering::Relaxed) {
                        return Err(Error::msg("Audio output error detected during write"));
                    }
                }
            }
        }

        self.vu_meter.maybe_send_event();

        Ok(())
    }

    pub fn flush(&mut self) {
        _ = self.stream.pause();
    }
}
