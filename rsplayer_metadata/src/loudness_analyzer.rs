use std::{fs::File, path::Path};

use symphonia::core::{
    codecs::{audio::AudioDecoderOptions, CodecParameters},
    errors::Error as SymphoniaError,
    formats::probe::Hint,
    formats::{FormatOptions, FormatReader, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

use crate::dsd_bundle::{build_codec_registry, build_probe, CODEC_TYPE_DSD_LSBF, CODEC_TYPE_DSD_MSBF};

pub struct LoudnessAnalyzer;

impl LoudnessAnalyzer {
    pub fn measure_file(file_path: &Path) -> Option<f64> {
        let file = Box::new(File::open(file_path).ok()?);
        let mss = MediaSourceStream::new(file, symphonia::core::io::MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = file_path.extension() {
            hint.with_extension(ext.to_str().unwrap_or(""));
        }

        let probe = build_probe();
        let mut format = probe
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .ok()?;

        Self::measure_from_format(&mut *format)
    }

    fn measure_from_format(format: &mut dyn FormatReader) -> Option<f64> {
        let track = format.default_track(TrackType::Audio)?;
        let track_id = track.id;

        let CodecParameters::Audio(audio_params) = track.codec_params.as_ref()? else {
            return None;
        };

        let codec = audio_params.codec;
        if codec == symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO
            || codec == CODEC_TYPE_DSD_LSBF
            || codec == CODEC_TYPE_DSD_MSBF
        {
            return None;
        }

        let channels = u32::try_from(audio_params.channels.as_ref()?.count()).ok()?;
        let sample_rate = audio_params.sample_rate?;

        let codec_registry = build_codec_registry();
        let mut decoder = codec_registry
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
            .ok()?;

        let mut meter = ebur128::EbuR128::new(channels, sample_rate, ebur128::Mode::I).ok()?;
        let mut sample_vec: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(Some(p)) => p,
                Err(SymphoniaError::ResetRequired) => {
                    decoder.reset();
                    continue;
                }
                Ok(None) | Err(_) => break,
            };

            if packet.track_id() != track_id {
                continue;
            }

            let Ok(audio_buf) = decoder.decode(&packet) else {
                continue;
            };
            audio_buf.copy_to_vec_interleaved(&mut sample_vec);
            let _ = meter.add_frames_f32(&sample_vec);
        }

        meter.loudness_global().ok()
    }
}
