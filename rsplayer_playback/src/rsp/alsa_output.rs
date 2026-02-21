use anyhow::{Error, Result};
use api_models::settings::RsPlayerSettings;
use log::info;
use std::sync::Arc;
use symphonia::core::audio::{AudioBufferRef, SignalSpec};

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

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
        error_flag: &AtomicBool,
    ) -> Result<()>;
}

struct PcmWriter<T>
where
    T: Sample,
{
    producer: rb::Producer<T>,
    sample_buf: SampleBuffer<T>,
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
        error_flag: &AtomicBool,
    ) -> Result<()> {
        let channels = decoded.spec().channels.count();
        self.sample_buf.copy_interleaved_ref(decoded);

        // Equalizer — skipped entirely when no filters are configured.
        if let Some(ref mut dsp) = dsp {
            if dsp.handle.has_filters.load(Ordering::Acquire) {
                let samples_mut = self.sample_buf.samples_mut();
                dsp.equalizer.process_samples(samples_mut);
            }
        }

        // VU metering.
        if let Some(ref mut vu) = vu_meter {
            let samples = self.sample_buf.samples();
            vu.update_peaks(channels, samples);
        }

        // Push to ring buffer.
        let mut remaining = self.sample_buf.samples();
        while let Ok(Some(written)) = self.producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            remaining = &remaining[written..];
            if error_flag.load(Ordering::Relaxed) {
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
        error_flag: &AtomicBool,
    ) -> Result<()> {
        // DSD — copy straight to ring buffer, no DSP or VU conversion.
        self.sample_buf.copy_interleaved_ref(decoded);
        let mut remaining = self.sample_buf.samples();
        while let Ok(Some(written)) = self.producer.write_blocking_timeout(remaining, Duration::from_secs(1)) {
            remaining = &remaining[written..];
            if error_flag.load(Ordering::Relaxed) {
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
    error_flag: Arc<AtomicBool>,
    /// DSP processing state — `None` when DSP is disabled or format is DSD.
    dsp: Option<DspState>,
    /// VU meter — `None` when VU metering is disabled.
    vu_meter: Option<VUMeter>,
}

#[allow(clippy::too_many_arguments)]
impl AlsaOutput {
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

        // Rebuild the equalizer for this track's spec.  Skip for DSD.
        let effective_dsp = if is_dsd {
            None
        } else if let Some(handle) = dsp_handle {
            handle.rebuild(spec.channels.count(), spec.rate as usize);
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
        )
    }

    fn open_with_format(
        spec: SignalSpec,
        duration: u64,
        device: &cpal::Device,
        rsp_settings: &RsPlayerSettings,
        dsp_handle: Option<DspHandle>,
        vu_meter: Option<VUMeter>,
        sample_format: cpal::SampleFormat,
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

        // Build the stream and format-specific writer in one match.
        // Each arm creates its own typed ring buffer and sample buffer.
        macro_rules! build_pcm_variant {
            ($T:ty) => {{
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
                let writer = Box::new(PcmWriter {
                    producer,
                    sample_buf: SampleBuffer::<$T>::new(duration, spec),
                });
                (stream, writer as Box<dyn AudioWriter>)
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
            error_flag,
            dsp,
            vu_meter,
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
            .write(decoded, &mut self.dsp, &mut self.vu_meter, &self.error_flag)?;

        if let Some(ref mut vu) = self.vu_meter {
            vu.maybe_send_event();
        }

        Ok(())
    }

    pub fn flush(&mut self) {
        _ = self.stream.pause();
    }
}
