use api_models::{settings::SpotifySettings, spotify::SpotifyAccountInfo};
use rspotify::{
    clients::{BaseClient, OAuthClient},
    scopes, AuthCodeSpotify, Config, Credentials, OAuth,
};

use crate::common::Result;

#[derive(Clone)]
pub struct SpotifyOauth {
    pub client: AuthCodeSpotify,
    pub settings: SpotifySettings,
}

impl SpotifyOauth {
    pub fn new(settings: SpotifySettings) -> Self {
        Self {
            client: create_oauth(&settings),
            settings: settings.clone(),
        }
    }

    pub fn get_authorization_url(&mut self) -> Result<String> {
        let url = self.client.get_authorize_url(true)?;
        Ok(url)
    }

    pub fn is_token_present(&mut self) -> Result<bool> {
        match self.client.read_token_cache(true) {
            Ok(Some(new_token)) => {
                let expired = new_token.is_expired();

                // Load token into client regardless of whether it's expired o
                // not, since it will be refreshed later anyway.
                *self.client.get_token().lock().unwrap() = Some(new_token);

                if expired {
                    // Ensure that we actually got a token from the refetch
                    match self.client.refetch_token()? {
                        Some(refreshed_token) => {
                            log::info!("Successfully refreshed expired token from token cache");
                            *self.client.get_token().lock().unwrap() = Some(refreshed_token)
                        }
                        // If not, prompt the user for it
                        None => {
                            log::info!("Unable to refresh expired token from token cache");
                            return Ok(false);
                        }
                    }
                }
            }
            // Otherwise following the usual procedure to get the token.
            _ => return Ok(false),
        }

        _ = self.client.write_token_cache();
        Ok(true)
    }

    pub fn authorize_callback(&mut self, code: &str) -> Result<()> {
        self.client.request_token(code)?;
        Ok(())
    }

    pub fn get_account_info(&mut self) -> SpotifyAccountInfo {
        if self.is_token_present().map_or(false, |r| r) {
            if let Ok(me) = self.client.me() {
                return SpotifyAccountInfo {
                    display_name: me.display_name,
                    email: me.email,
                    image_url: me
                        .images
                        .and_then(|imgs| imgs.first().map(|i| i.url.clone())),
                };
            } else {
                return SpotifyAccountInfo::default();
            }
        }
        SpotifyAccountInfo::default()
    }
}

pub fn create_oauth(settings: &SpotifySettings) -> AuthCodeSpotify {
    let cred = Credentials::new(
        settings.developer_client_id.as_str(),
        settings.developer_secret.as_str(),
    );
    let oauth = OAuth {
        redirect_uri: settings.auth_callback_url.clone(),
        scopes: scopes!(
            "user-read-currently-playing",
            "playlist-modify-private",
            "playlist-read-private",
            "user-read-recently-played",
            "user-modify-playback-state",
            "user-read-playback-state"
        ),
        ..Default::default()
    };
    let config = Config {
        token_cached: true,
        token_refreshing: true,
        ..Default::default()
    };
    AuthCodeSpotify::with_config(cred, oauth, config)
}

// Overview
// Images
// ugc-image-upload
// Spotify Connect
// user-modify-playback-state
// user-read-playback-state
// user-read-currently-playing
// Follow
// user-follow-modify
// user-follow-read

// Listening History
// user-read-recently-played
// user-read-playback-position
// user-top-read

// Playlists
// playlist-read-collaborative
// playlist-modify-public
// playlist-read-private
// playlist-modify-private

// Playback
// app-remote-control
// streaming

// Users
// user-read-email
// user-read-private

// Library
// user-library-modify
// user-library-read
