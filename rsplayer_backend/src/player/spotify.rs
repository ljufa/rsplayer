use std::process::Child;
use std::time::Duration;

use api_models::player::Song;
use api_models::playlist::{
    Album, Category, DynamicPlaylistsPage, Playlist, PlaylistPage, PlaylistType, Playlists,
};
use api_models::settings::{SpotifySettings, AlsaSettings};
use api_models::state::{
    PlayerInfo, PlayerState, PlayingContext, PlayingContextQuery, PlayingContextType, SongProgress,
};
use failure::err_msg;
use log::info;

use rspotify::clients::{BaseClient, OAuthClient};
use rspotify::model::{
    AlbumId, CurrentUserQueue, Id, Market, Offset, PlayableId, PlayableItem, PlaylistId,
    SimplifiedAlbum, TrackId,
};
use rspotify::prelude::PlayContextId;

use crate::common::Result;
use crate::config::Configuration;
use crate::player::Player;

use super::spotify_oauth::SpotifyOauth;

pub struct SpotifyPlayerClient {
    librespot_process: Option<Child>,
    oauth: SpotifyOauth,
    device_id: Option<String>,
    playing_item: Option<Song>,
    playing_context: Option<PlayingContext>,
    progress: Option<Duration>,
    playlist_group: Option<Playlists>,
    force_context_update: bool,
}

impl SpotifyPlayerClient {
    pub fn new(settings: &SpotifySettings) -> Result<SpotifyPlayerClient> {
        if !settings.enabled {
            return Err(err_msg("Spotify integration is disabled."));
        }
        let mut client = SpotifyOauth::new(settings);
        if !client.is_token_present()? {
            return Err(err_msg(
                "Spotify token not found, please complete configuration",
            ));
        }
        Ok(SpotifyPlayerClient {
            oauth: client,
            librespot_process: None,
            device_id: None,
            playing_item: None,
            playing_context: None,
            progress: None,
            playlist_group: None,
            force_context_update: false,
        })
    }

    pub fn start_device(&mut self, alsa_device_name: &str) -> Result<()> {
        self.librespot_process = Some(start_librespot(&self.oauth.settings, alsa_device_name)?);
        Ok(())
    }

    pub fn transfer_playback_to_device(&mut self) -> Result<()> {
        let mut dev = "".to_string();
        let mut tries = 0;
        let device_name = self.oauth.settings.device_name.as_str();
        while tries < 15 {
            for d in self.oauth.client.device()? {
                if d.name.contains(device_name) {
                    let device_id = d.id.as_ref();
                    if device_id.is_some() && !d.is_active {
                        self.oauth
                            .client
                            .transfer_playback(device_id.unwrap().as_str(), Some(false))?;
                    }
                    dev = device_id.unwrap().clone();
                }
            }
            if dev.is_empty() {
                tries += 1;
                warn!("Spotify Device not found. Retry:{}", tries);
                std::thread::sleep(Duration::from_millis(500));
            } else {
                break;
            }
        }
        if dev.is_empty() {
            error!("Spotify device not found: {}", device_name);
            return Err(err_msg(format!(
                "Spotify device {} not found!",
                device_name
            )));
        }
        info!("Spotify client created sucessfully!");
        self.device_id = Some(dev);
        Ok(())
    }

    fn update_playing_context(&mut self, context: Option<&rspotify::model::Context>) {
        if let Some(ctx) = context {
            if self.playing_context.is_none()
                || self.playing_context.as_ref().unwrap().id != ctx.uri
                || self.force_context_update
            {
                debug!("Update playing context!");
                self.playing_context = Some(self.fetch_playing_context(ctx));
                if self.force_context_update {
                    self.force_context_update = false;
                }
            }
        } else {
            self.playing_context = None;
        }
    }

    fn update_playing_item(&mut self, context: Option<&PlayableItem>) {
        if let Some(it) = context {
            if self.playing_item.is_none()
                || self.playing_item.as_ref().unwrap().id
                    != it.id().map_or("".to_string(), |id| id.id().to_string())
            {
                self.playing_item = playable_item_to_song(Some(it));
            }
        }
    }

    fn fetch_playing_context(
        &mut self,
        context: &rspotify::model::Context,
    ) -> PlayingContext {
        let queue = self.oauth.client.current_user_queue().ok();
        PlayingContext {
            id: context.uri.clone(),
            name: "Queue".to_string(),
            player_type: api_models::common::PlayerType::SPF,
            context_type: api_models::state::PlayingContextType::Playlist {
                description: None,
                public: None,
                snapshot_id: "1".to_string(),
            },
            playlist_page: queue.map(|q| queue_to_page(&q)),
            image_url: None,
        }
    }
}

impl Drop for SpotifyPlayerClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Player for SpotifyPlayerClient {
    fn play(&mut self) {
        let play = self
            .oauth
            .client
            .resume_playback(self.device_id.as_deref(), None);
        if play.is_err() {
            _ = self.transfer_playback_to_device();
            _ = self
                .oauth
                .client
                .resume_playback(self.device_id.as_deref(), None);
        }
    }

    fn pause(&mut self) {
        _ = self.oauth.client.pause_playback(self.device_id.as_deref());
    }
    fn next_track(&mut self) {
        _ = self.oauth.client.next_track(self.device_id.as_deref());
    }
    fn prev_track(&mut self) {
        _ = self.oauth.client.previous_track(self.device_id.as_deref());
    }
    fn stop(&mut self) {
        _ = self.oauth.client.pause_playback(self.device_id.as_deref());
    }

    fn shutdown(&mut self) {
        info!("Shutting down Spotify player!");
        if self.device_id.is_some() {
            self.stop();
        }
        _ = self.librespot_process.as_mut().unwrap().kill();
    }

    fn rewind(&mut self, _seconds: i8) {}

    fn random_toggle(&mut self) {
        if let Some(pi) = self.get_player_info() {
            let current_shuffle = pi.random.unwrap_or_default();
            _ = self.oauth.client.shuffle(!current_shuffle, None);
        }
    }

    fn load_playlist(&mut self, pl_id: String) {
        _ = self.oauth.client.start_context_playback(
            PlayContextId::Playlist(PlaylistId::from_id_or_uri(pl_id.as_str()).unwrap()), //todo remove unwrap
            self.device_id.as_deref(),
            None,
            None,
        );
    }
    fn load_album(&mut self, album_id: String) {
        _ = self.oauth.client.start_context_playback(
            PlayContextId::Album(AlbumId::from_id_or_uri(album_id.as_str()).unwrap()), //todo remove unwrap
            self.device_id.as_deref(),
            None,
            None,
        );
    }

    fn play_item(&mut self, id: String) {
        if let Some(ctx) = &self.playing_context {
            match &ctx.context_type {
                PlayingContextType::Playlist { .. } => {
                    _ = self.oauth.client.start_context_playback(
                        PlayContextId::Playlist(
                            PlaylistId::from_id_or_uri(ctx.id.as_str()).unwrap(),
                        ),
                        None,
                        Some(Offset::Uri(format!("spotify:track:{id}"))),
                        None,
                    );
                }
                PlayingContextType::Album { .. } => {
                    _ = self.oauth.client.start_context_playback(
                        PlayContextId::Album(AlbumId::from_id_or_uri(ctx.id.as_str()).unwrap()),
                        None,
                        Some(Offset::Uri(format!("spotify:track:{id}"))),
                        None,
                    );
                }
                _ => {}
            }
        } else {
            _ = self.oauth.client.start_uris_playback(
                [PlayableId::Track(
                    TrackId::from_id_or_uri(&id).expect("Unable to convert id to uri"),
                )],
                self.device_id.as_deref(),
                None,
                None,
            );
        }
    }

    fn remove_playlist_item(&mut self, id: String) {
        if let Some(pc) = self.playing_context.as_mut() {
            if let PlayingContextType::Playlist { snapshot_id, .. } = &pc.context_type {
                let track_id = PlayableId::Track(TrackId::from_id_or_uri(id.as_str()).unwrap());
                let track_ids = vec![track_id];
                match self.oauth.client.playlist_remove_all_occurrences_of_items(
                    PlaylistId::from_id_or_uri(pc.id.as_str()).unwrap(),
                    track_ids,
                    Some(snapshot_id),
                ) {
                    Ok(_) => {
                        if let Some(pc) = pc.playlist_page.as_mut() {
                            pc.remove_item(&id);
                        }
                    }
                    Err(e) => error!("Failed to delete item {id} from playlist:{e}"),
                }
            }
        }
    }

    fn get_song_progress(&mut self) -> SongProgress {
        let total_time = self
            .playing_item
            .as_ref()
            .map(|p| p.time.unwrap_or_default())
            .unwrap_or_default();
        let prog = self.progress.unwrap_or_default();
        SongProgress {
            total_time,
            current_time: prog,
        }
    }

    fn get_current_song(&mut self) -> Option<Song> {
        self.playing_item.clone()
    }

    fn get_player_info(&mut self) -> Option<PlayerInfo> {
        if let Ok(Some(playback_ctx)) = self.oauth.client.current_playback(None, None::<&[_]>) {
            self.update_playing_item(playback_ctx.item.as_ref());
            self.update_playing_context(playback_ctx.context.as_ref());
            self.progress = playback_ctx.progress;
            Some(PlayerInfo {
                random: Some(playback_ctx.shuffle_state),
                state: if playback_ctx.is_playing {
                    Some(PlayerState::PLAYING)
                } else {
                    Some(PlayerState::PAUSED)
                },
                audio_format_rate: Option::default(),
                audio_format_bit: Option::default(),
                audio_format_channels: Option::default(),
            })
        } else {
            None
        }
    }

    fn get_playing_context(&mut self, query: PlayingContextQuery) -> Option<PlayingContext> {
        self.playing_context.as_ref().map(|context| PlayingContext {
            context_type: context.context_type.clone(),
            id: context.id.clone(),
            image_url: context.image_url.clone(),
            name: context.name.clone(),
            player_type: context.player_type,
            playlist_page: match query {
                PlayingContextQuery::WithSearchTerm(term, offset) => {
                    if term.is_empty() {
                        return context.clone();
                    }
                    context.playlist_page.as_ref().map(|pp| PlaylistPage {
                        total: 0,
                        offset,
                        limit: 0,
                        items: pp
                            .items
                            .iter()
                            .filter(|s| s.all_text().to_lowercase().contains(&term.to_lowercase()))
                            .cloned()
                            .collect(),
                    })
                }
                PlayingContextQuery::CurrentSongPage | PlayingContextQuery::IgnoreSongs => None,
            },
        })
    }

    fn get_playlist_categories(&mut self) -> Vec<Category> {
        let categories = self.oauth.client.categories_manual(
            Some("en_DE"),
            Some(Market::FromToken),
            Some(50),
            Some(0),
        );

        if let Ok(categories) = categories {
            let mut result: Vec<Category> = categories
                .items
                .iter()
                .map(|c| Category {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    icon: c.icons.first().map_or("".to_string(), |i| i.url.clone()),
                })
                .collect();
            result.dedup();
            result.sort();
            result
        } else {
            error!("Failed to get categories:{}", categories.unwrap_err());
            vec![]
        }
    }

    fn get_static_playlists(&mut self) -> Playlists {
        if self.playlist_group.is_none() {
            // get featured
            let featured = self.oauth.client.featured_playlists(
                None,
                Some(Market::FromToken),
                None,
                Some(20),
                Some(0),
            );
            let mut items = featured
                .map(|r| simplified_playlist_to_playlist_type(&r))
                .unwrap_or_default();

            // get new releases
            let new_releases =
                self.oauth
                    .client
                    .new_releases_manual(Some(Market::FromToken), Some(20), Some(0));
            if let Ok(releases) = new_releases {
                for a in &releases.items {
                    items.push(album_to_playlist_type(a));
                }
            }

            // get user's playlists
            if let Ok(page) = self
                .oauth
                .client
                .current_user_playlists_manual(Some(20), Some(0))
            {
                for sp in &page.items {
                    items.push(PlaylistType::Saved(Playlist {
                        name: sp.name.clone(),
                        id: sp.id.to_string(),
                        description: None,
                        image: sp.images.first().map(|i| i.url.clone()),
                        owner_name: sp.owner.display_name.clone(),
                    }));
                }
            }
            self.playlist_group = Some(Playlists { items });
        }
        self.playlist_group
            .as_ref()
            .map_or(Playlists::default(), std::clone::Clone::clone)
    }

    fn get_dynamic_playlists(
        &mut self,
        category_ids: Vec<String>,
        offset: u32,
        limit: u32,
    ) -> Vec<DynamicPlaylistsPage> {
        let mut result = vec![];
        for cat_id in &category_ids {
            let cat_pls = self.oauth.client.category_playlists_manual(
                cat_id,
                Some(Market::FromToken),
                Some(limit),
                Some(offset),
            );
            let mut items = vec![];
            if let Ok(cat_pls) = cat_pls {
                for pl in &cat_pls.items {
                    items.push(Playlist {
                        id: pl.id.to_string(),
                        name: pl.name.clone(),
                        image: pl.images.first().map(|i| i.url.clone()),
                        owner_name: pl.owner.display_name.clone(),
                        description: None,
                    });
                }
            }
            result.push(DynamicPlaylistsPage {
                category_id: cat_id.clone(),
                playlists: items,
                offset,
                limit,
            });
        }

        result
    }

    fn get_playlist_items(&mut self, playlist_id: String) -> Vec<Song> {
        let items = self.oauth.client.playlist_items_manual(
            PlaylistId::from_id_or_uri(&playlist_id).unwrap(),
            None,
            None,
            Some(100),
            Some(0),
        );
        if let Ok(pg) = items {
            pg.items
                .iter()
                .map(|i| playable_item_to_song(i.track.as_ref()).unwrap())
                .collect()
        } else {
            vec![]
        }
    }

    fn load_song(&mut self, _song_id: String) {
        // todo!()
    }

    fn add_song_to_queue(&mut self, song_id: String) {
        if let Ok(track_id) = TrackId::from_id_or_uri(song_id.as_str()) {
            _ = self
                .oauth
                .client
                .add_item_to_queue(PlayableId::Track(track_id), None);
            self.force_context_update = true;
        }
    }

    fn clear_queue(&mut self) {
        // todo!()
    }

    fn save_queue_as_playlist(&mut self, _playlist_name: String) {
        // todo!()
    }
}

fn simplified_playlist_to_playlist_type(
    pl: &rspotify::model::FeaturedPlaylists,
) -> Vec<PlaylistType> {
    let res = pl
        .playlists
        .items
        .iter()
        .map(|pl| {
            PlaylistType::Featured(Playlist {
                id: pl.id.to_string(),
                name: pl.name.clone(),
                image: pl.images.first().map(|i| i.url.clone()),
                owner_name: pl.owner.display_name.clone(),
                description: None,
            })
        })
        .collect();
    res
}

fn album_to_playlist_type(album: &SimplifiedAlbum) -> PlaylistType {
    PlaylistType::NewRelease(Album {
        id: album
            .id
            .as_ref()
            .map_or("".to_string(), std::string::ToString::to_string),
        album_name: album.name.clone(),
        album_type: album
            .album_type
            .as_ref()
            .map_or("".to_string(), std::clone::Clone::clone),
        images: album.images.iter().map(|i| i.url.clone()).collect(),
        artists: album.artists.iter().map(|a| a.name.clone()).collect(),
        genres: vec![],
        release_date: album.release_date.clone(),
    })
}

fn queue_to_page(tracks: &CurrentUserQueue) -> PlaylistPage {
    let items: Vec<Song> = tracks
        .queue
        .iter()
        .map_while(|tr| playable_item_to_song(Some(tr)))
        .collect();
    PlaylistPage {
        total: tracks.queue.len(),
        offset: 0,
        limit: 0,
        items,
    }
}

fn playable_item_to_song(track: Option<&PlayableItem>) -> Option<Song> {
    match track {
        Some(rspotify::model::PlayableItem::Track(track)) => Some(Song {
            id: track
                .id
                .as_ref()
                .map_or("".to_string(), |id| id.id().to_string()),
            album: Some(track.album.name.clone()),
            artist: track.artists.first().map(|a| a.name.clone()),
            genre: None,
            date: track.album.release_date.clone(),
            file: track
                .href
                .as_ref()
                .map_or("".to_string(), std::clone::Clone::clone),
            title: Some(track.name.clone()),
            time: Some(track.duration),
            image_url: track.album.images.first().map(|i| i.url.clone()),
            ..Default::default()
        }),
        _ => None,
    }
}

fn start_librespot(settings: &SpotifySettings, alsa_device_name: &str) -> Result<Child> {
    info!("Starting librespot process");
    let format: &'static str = settings.alsa_device_format.into();
    let child = std::process::Command::new(Configuration::get_librespot_path())
        .arg("--disable-audio-cache")
        .arg("--bitrate")
        .arg(settings.bitrate.to_string())
        .arg("--name")
        .arg(settings.device_name.clone())
        .arg("--backend")
        .arg("alsa")
        .arg("--username")
        .arg(settings.username.clone())
        .arg("--password")
        .arg(settings.password.clone())
        .arg("--device")
        .arg(alsa_device_name)
        .arg("--format")
        .arg(format)
        .arg("--initial-volume")
        .arg("100")
        .arg("--autoplay")
        .spawn();
    match child {
        Ok(c) => Ok(c),
        Err(e) => Err(failure::format_err!(
            "Can't start librespot process. Error: {}",
            e
        )),
    }
}
