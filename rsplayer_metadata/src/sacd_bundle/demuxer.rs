use std::fs::File;
use std::num::NonZero;

use symphonia::core::audio::{Channels, Position};
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::common::FourCc;
use symphonia::core::errors::{Error, Result};
use symphonia::core::formats::{FormatId, FormatInfo, FormatReader, SeekMode, SeekTo, SeekedTo, Track, TrackFlags};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{Metadata, MetadataLog};
use symphonia::core::packet::Packet;
use symphonia::core::units::{Duration, TimeBase, Timestamp};

use super::scarletbook::{
    audio_bytes_per_sector, detect_sector_mode, read_areas, read_audio_sectors, read_tracks, SectorMode,
    SACD_SAMPLING_FREQUENCY,
};
use crate::dsd_bundle::CODEC_TYPE_DSD_MSBF;

pub const SACD_ISO_FORMAT_INFO: FormatInfo = FormatInfo {
    format: FormatId::new(FourCc::new(*b"SACD")),
    short_name: "sacd_iso",
    long_name: "SACD ISO (Scarletbook)",
};

/// Number of 2048-byte sectors to read per audio packet.
const SECTORS_PER_PACKET: u64 = 32;

pub struct SacdIsoReader {
    file: File,
    tracks: Vec<Track>,
    metadata: MetadataLog,
    sector_mode: SectorMode,
    channel_count: usize,
    frame_format: u8,
    audio_bytes_per_sector: u64,
    track_start_sector: u64,
    current_sector: u64,
    track_end_sector: u64,
    current_pts: u64,
}

impl SacdIsoReader {
    /// Open an SACD ISO and position for playback of the given 0-based track index.
    /// Stereo area is preferred; falls back to the first available area.
    pub fn try_new_for_track(mut file: File, track_idx: usize) -> anyhow::Result<Self> {
        let mode = detect_sector_mode(&mut file)?;
        let areas = read_areas(&mut file, mode)?;

        let area = areas
            .iter()
            .find(|a| a.is_stereo)
            .or_else(|| areas.first())
            .ok_or_else(|| anyhow::anyhow!("No playable area found in SACD ISO"))?;

        let tracks_list = read_tracks(&mut file, mode, area)?;

        if track_idx >= tracks_list.len() {
            return Err(anyhow::anyhow!(
                "Track index {} out of range (ISO has {} tracks)",
                track_idx,
                tracks_list.len()
            ));
        }

        let sacd_track = &tracks_list[track_idx];
        let channel_count = area.channel_count as usize;
        let channel_count_nonzero = channel_count.max(1);
        let ff = area.frame_format;
        let abps = audio_bytes_per_sector(ff);

        let channels = match channel_count {
            1 => Channels::Positioned(Position::FRONT_CENTER),
            6 => Channels::Positioned(
                Position::FRONT_LEFT
                    | Position::FRONT_RIGHT
                    | Position::FRONT_CENTER
                    | Position::LFE1
                    | Position::REAR_LEFT
                    | Position::REAR_RIGHT,
            ),
            _ => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT),
        };

        let samples_per_packet = SECTORS_PER_PACKET * abps / channel_count_nonzero as u64 * 8;
        let total_dsd_frames = sacd_track.total_dsd_frames(area.channel_count, ff);

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_TYPE_DSD_MSBF)
            .with_sample_rate(SACD_SAMPLING_FREQUENCY)
            .with_channels(channels)
            .with_frames_per_block(samples_per_packet);

        let mut track = Track::new(0);
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_num_frames(total_dsd_frames)
            .with_flags(TrackFlags::DEFAULT);

        Ok(SacdIsoReader {
            file,
            tracks: vec![track],
            metadata: MetadataLog::default(),
            sector_mode: mode,
            channel_count: channel_count_nonzero,
            frame_format: ff,
            audio_bytes_per_sector: abps,
            track_start_sector: u64::from(sacd_track.start_lsn),
            current_sector: u64::from(sacd_track.start_lsn),
            track_end_sector: u64::from(sacd_track.start_lsn) + u64::from(sacd_track.length_lsn),
            current_pts: 0,
        })
    }
}

impl FormatReader for SacdIsoReader {
    fn format_info(&self) -> &FormatInfo {
        &SACD_ISO_FORMAT_INFO
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        if self.current_sector >= self.track_end_sector {
            return Ok(None);
        }

        let sectors = SECTORS_PER_PACKET.min(self.track_end_sector - self.current_sector);

        let data = read_audio_sectors(
            &mut self.file,
            self.sector_mode,
            self.current_sector,
            sectors,
            self.channel_count,
            self.frame_format,
        )
        .map_err(Error::IoError)?;

        let pts = Timestamp::new(self.current_pts.cast_signed());
        let actual_frames = sectors * self.audio_bytes_per_sector / self.channel_count as u64 * 8;
        let dur = Duration::new(actual_frames);

        self.current_sector += sectors;
        self.current_pts += actual_frames;

        Ok(Some(Packet::new(0, pts, dur, data)))
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let tb = TimeBase::try_from_recip(SACD_SAMPLING_FREQUENCY).unwrap_or_else(|| {
            TimeBase::new(
                NonZero::new(1).expect("nonzero"),
                NonZero::new(SACD_SAMPLING_FREQUENCY).expect("nonzero"),
            )
        });

        let total_frames = self.tracks[0].num_frames.unwrap_or(0);
        let required_ts = match to {
            SeekTo::TimeStamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => tb.calc_timestamp(time).unwrap_or(Timestamp::ZERO),
        };

        let max_ts = Timestamp::new(total_frames.cast_signed());
        let clamped_ts = required_ts.min(max_ts);
        let target_frame = clamped_ts.get().unsigned_abs();

        // Align to sector boundary.
        let frames_per_sector = self.audio_bytes_per_sector / self.channel_count as u64 * 8;
        let sector_offset = target_frame / frames_per_sector;
        let actual_frame = sector_offset * frames_per_sector;

        self.current_sector = self.track_start_sector + sector_offset;
        self.current_pts = actual_frame;

        Ok(SeekedTo {
            track_id: 0,
            required_ts,
            actual_ts: Timestamp::new(actual_frame.cast_signed()),
        })
    }

    fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
    where
        Self: 's,
    {
        unreachable!("SacdIsoReader does not use MediaSourceStream")
    }
}
