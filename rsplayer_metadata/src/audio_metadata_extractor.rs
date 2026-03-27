use std::time::Duration;

use api_models::player::Song;
use symphonia::core::{
    formats::{FormatReader, TrackType},
    meta::StandardTag,
};

pub struct AudioMetadataExtractor;

impl AudioMetadataExtractor {
    pub fn extract(format: &mut dyn FormatReader) -> (Song, Option<symphonia::core::meta::Visual>) {
        let mut song = Song::default();
        let mut image_data: Option<symphonia::core::meta::Visual> = None;

        if let Some(track) = format.default_track(TrackType::Audio) {
            if let Some(num_frames) = track.num_frames {
                if let Some(tb) = track.time_base {
                    if let Some(time) = tb.calc_time(symphonia::core::units::Timestamp::new(num_frames.cast_signed())) {
                        song.time = Some(Duration::from_secs(time.as_secs().unsigned_abs()));
                    }
                }
            }
        }

        if let Some(metadata_rev) = format.metadata().skip_to_latest() {
            Self::fill_song_from_metadata(metadata_rev, &mut song, &mut image_data);
        }

        (song, image_data)
    }

    fn fill_song_from_metadata(
        metadata_rev: &symphonia::core::meta::MetadataRevision,
        song: &mut Song,
        image_data: &mut Option<symphonia::core::meta::Visual>,
    ) {
        let tags = &metadata_rev.media.tags;

        for tag in tags.iter().filter(|t| t.has_std_tag()) {
            if let Some(std_tag) = &tag.std {
                match std_tag {
                    StandardTag::Album(_) => {
                        if song.album.is_none() {
                            song.album = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::AlbumArtist(_) => {
                        if song.album_artist.is_none() {
                            song.album_artist = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::Artist(_) => {
                        if song.artist.is_none() {
                            song.artist = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::Composer(_) => {
                        if song.composer.is_none() {
                            song.composer = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::RecordingDate(_)
                    | StandardTag::ReleaseDate(_)
                    | StandardTag::RecordingYear(_)
                    | StandardTag::ReleaseYear(_) => {
                        if song.date.is_none() {
                            song.date = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::DiscNumber(_) => {
                        if song.disc.is_none() {
                            song.disc = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::Genre(_) => {
                        if song.genre.is_none() {
                            song.genre = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::Label(_) => {
                        if song.label.is_none() {
                            song.label = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::Performer(_) => {
                        if song.performer.is_none() {
                            song.performer = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::TrackNumber(_) => {
                        if song.track.is_none() {
                            song.track = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    StandardTag::TrackTitle(_) => {
                        if song.title.is_none() {
                            song.title = Some(Self::tag_value_to_option(tag));
                        }
                    }
                    _ => {}
                }
            }
        }

        for tag in tags.iter().filter(|t| !t.has_std_tag()) {
            song.tags
                .entry(tag.raw.key.clone())
                .or_insert_with(|| Self::tag_value_to_option(tag));
        }

        if image_data.is_none() {
            if let Some(v) = metadata_rev.media.visuals.first() {
                *image_data = Some(v.clone());
            }
        }
    }

    fn tag_value_to_option(tag: &symphonia::core::meta::Tag) -> String {
        tag.raw.value.to_string()
    }
}
