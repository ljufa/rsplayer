use std::io::{Seek, SeekFrom};
use std::num::NonZero;

use symphonia::core::audio::{Channels, Position};
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::common::FourCc;
use symphonia::core::errors::Result;
use symphonia::core::formats::probe::{ProbeDataMatchSpec, ProbeFormatData, ProbeableFormat, Score, Scoreable};
use symphonia::core::formats::{FormatId, FormatInfo, FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo, Track};
use symphonia::core::io::{MediaSourceStream, ReadBytes, ScopedStream};
use symphonia::core::meta::{Metadata, MetadataBuilder, MetadataLog, MetadataSideData};
use symphonia::core::packet::Packet;
use symphonia::core::units::{Duration, TimeBase, Timestamp};

use symphonia_metadata::id3v2::{read_id3v2, ID3V2_METADATA_INFO};

use super::{dsf::DSFMetadata, CODEC_TYPE_DSD_LSBF};

const DSF_FORMAT_ID: FormatId = FormatId::new(FourCc::new(*b"DSF "));
const DSF_FORMAT_INFO: FormatInfo = FormatInfo {
    format: DSF_FORMAT_ID,
    short_name: "dsf",
    long_name: "DSD Stream File",
};

pub struct DsfReader<'s> {
    reader: MediaSourceStream<'s>,
    tracks: Vec<Track>,
    metadata: MetadataLog,
    data_start: u64,
    #[allow(dead_code)]
    data_end: u64,
    #[allow(dead_code)]
    block_size_per_channel: u32,
    #[allow(dead_code)]
    channel_num: u32,
    total_blocks: u64,
    current_block: u64,
    bytes_per_sample_frame: u32,
    samples_per_block: u64,
}

impl<'s> DsfReader<'s> {
    pub fn try_new(mut source: MediaSourceStream<'s>, _options: FormatOptions) -> Result<Self> {
        let dsf_metadata = DSFMetadata::read(&mut source)?;

        let data_start = source.pos();
        let data_len = dsf_metadata.data_chunk.chunk_size.saturating_sub(12);
        let data_end = data_start + data_len;

        let block_size = dsf_metadata.fmt_chunk.block_size_per_channel;
        let channels = dsf_metadata.fmt_chunk.channel_num;
        let sample_rate = dsf_metadata.fmt_chunk.sampling_frequency;
        let total_samples = dsf_metadata.fmt_chunk.sample_count;

        let channel_set = match dsf_metadata.fmt_chunk.channel_type {
            1 => Channels::Positioned(Position::FRONT_CENTER),
            7 => Channels::Positioned(
                Position::FRONT_LEFT
                    | Position::FRONT_RIGHT
                    | Position::FRONT_CENTER
                    | Position::LFE1
                    | Position::REAR_LEFT
                    | Position::REAR_RIGHT,
            ),
            _ => Channels::Positioned(Position::FRONT_LEFT | Position::FRONT_RIGHT),
        };

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_TYPE_DSD_LSBF)
            .with_sample_rate(sample_rate)
            .with_channels(channel_set)
            .with_frames_per_block(u64::from(block_size) * 8);

        let samples_per_block = u64::from(block_size) * 8;
        let bytes_per_sample_frame = block_size * channels;
        let total_blocks = if bytes_per_sample_frame > 0 {
            data_len / u64::from(bytes_per_sample_frame)
        } else {
            0
        };

        let mut track = Track::new(0);
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_num_frames(total_samples);

        let mut metadata_log = MetadataLog::default();
        let metadata_offset = dsf_metadata.dsd_chunk.metadata_offset;
        if metadata_offset > 0 {
            if let Ok(pos) = source.seek(SeekFrom::Start(metadata_offset)) {
                if pos == metadata_offset {
                    let mut metadata_builder = MetadataBuilder::new(ID3V2_METADATA_INFO);
                    let mut side_data: Vec<MetadataSideData> = Vec::new();
                    if let Err(e) = read_id3v2(&mut source, &mut metadata_builder, &mut side_data) {
                        log::warn!("Failed to read ID3v2 metadata from DSF: {e}");
                    } else {
                        metadata_log.push(metadata_builder.build());
                    }
                }
            }
            source.seek(SeekFrom::Start(data_start))?;
        }

        Ok(DsfReader {
            reader: source,
            tracks: vec![track],
            metadata: metadata_log,
            data_start,
            data_end,
            block_size_per_channel: block_size,
            channel_num: channels,
            total_blocks,
            current_block: 0,
            bytes_per_sample_frame,
            samples_per_block,
        })
    }
}

impl Scoreable for DsfReader<'_> {
    fn score(mut src: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
        let marker = src.read_quad_bytes()?;
        if &marker == b"DSD " {
            Ok(Score::Supported(255))
        } else {
            Ok(Score::Unsupported)
        }
    }
}

impl ProbeableFormat<'_> for DsfReader<'_> {
    fn try_probe_new(mss: MediaSourceStream<'_>, opts: FormatOptions) -> Result<Box<dyn FormatReader + '_>> {
        Ok(Box::new(DsfReader::try_new(mss, opts)?))
    }

    fn probe_data() -> &'static [ProbeFormatData] {
        const DATA: &[ProbeFormatData] = &[ProbeFormatData {
            info: DSF_FORMAT_INFO,
            spec: ProbeDataMatchSpec {
                extensions: &["dsf"],
                mime_types: &["audio/dsd", "application/x-dsd", "audio/x-dsf"],
                markers: &[b"DSD "],
            },
        }];
        DATA
    }
}

impl FormatReader for DsfReader<'_> {
    fn format_info(&self) -> &FormatInfo {
        &DSF_FORMAT_INFO
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        if self.current_block >= self.total_blocks {
            return Ok(None);
        }

        let pts = Timestamp::new((self.current_block * self.samples_per_block).cast_signed());
        let dur = Duration::new(self.samples_per_block);
        let packet_size = self.bytes_per_sample_frame as usize;
        let buf = self.reader.read_boxed_slice(packet_size)?;

        self.current_block += 1;

        Ok(Some(Packet::new(0, pts, dur, buf)))
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let sample_rate = match &self.tracks[0].codec_params {
            Some(CodecParameters::Audio(params)) => params.sample_rate.unwrap_or(2_822_400),
            _ => 2_822_400,
        };

        let tb = TimeBase::try_from_recip(sample_rate)
            .unwrap_or_else(|| TimeBase::new(NonZero::new(1).unwrap(), NonZero::new(sample_rate).unwrap()));

        let required_ts = match to {
            SeekTo::TimeStamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => tb.calc_timestamp(time).unwrap_or(Timestamp::ZERO),
        };

        let max_ts = Timestamp::new(self.total_blocks.saturating_mul(self.samples_per_block).cast_signed());
        let clamped_ts = required_ts.min(max_ts);
        let block_index = clamped_ts.get().unsigned_abs() / self.samples_per_block;
        let byte_offset = block_index * u64::from(self.bytes_per_sample_frame);
        let abs_pos = self.data_start + byte_offset;

        self.reader.seek(SeekFrom::Start(abs_pos))?;
        self.current_block = block_index;

        let actual_ts = Timestamp::new((block_index * self.samples_per_block).cast_signed());

        Ok(SeekedTo {
            track_id: 0,
            actual_ts,
            required_ts,
        })
    }

    fn into_inner<'r>(self: Box<Self>) -> MediaSourceStream<'r>
    where
        Self: 'r,
    {
        self.reader
    }
}
