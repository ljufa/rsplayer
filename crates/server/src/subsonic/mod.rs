//! Subsonic / `OpenSubsonic` compatible API under `/rest/`.
//!
//! Lets third-party Subsonic apps (Symfonium, `DSub`, Amperfy, play:Sub,
//! Feishin, Supersonic, ...) browse the library and stream its files. One
//! user, configured in [`api_models::settings::SubsonicSettings`]; the
//! routes are always mounted and answer with a Subsonic error while the API
//! is disabled, so toggling it needs no restart. Endpoints are reachable as
//! `/rest/name` and `/rest/name.view`, by GET or form POST.
//!
//! - [`auth`]: password and token authentication.
//! - [`response`]: one response model rendered as XML (the default), JSON or
//!   JSONP.
//! - [`ids`]: opaque ids derived from library keys.
//! - [`library`]: browsing, lists, search, playlists and annotation over the
//!   same repositories as [`crate::browse_api`], run on blocking threads
//!   since they read fjall.
//! - [`playlists`]: saved playlists plus Most Played and Liked.
//! - [`media`]: `stream`/`download` (original files, no transcoding, with
//!   range requests, cue tracks and remote libraries) and `getCoverArt`.
//!
//! Jukebox control, transcoding, user management, podcasts, radio, shares
//! and bookmarks are not provided.

mod auth;
mod ids;
mod library;
mod media;
mod playlists;
mod response;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path as AxumPath, RawQuery, State},
    http::{HeaderMap, Method},
    response::Response,
    routing::get,
    Router,
};
use tokio::sync::mpsc;
use tower_http::compression::{
    predicate::{DefaultPredicate, NotForContentType, Predicate},
    CompressionLayer,
};

use api_models::common::UserCommand;
use config::Configuration;
use metadata::ports::play_statistics_repository::ArcPlayStatisticsRepository;

use crate::browse_api::BrowseState;
use response::{ApiError, Format};

#[derive(Clone)]
pub struct SubsonicState {
    pub(crate) browse: BrowseState,
    pub(crate) statistics: ArcPlayStatisticsRepository,
    pub(crate) config: Arc<Configuration>,
    pub(crate) user_commands_tx: mpsc::Sender<UserCommand>,
}

impl SubsonicState {
    pub const fn new(
        browse: BrowseState,
        statistics: ArcPlayStatisticsRepository,
        config: Arc<Configuration>,
        user_commands_tx: mpsc::Sender<UserCommand>,
    ) -> Self {
        Self {
            browse,
            statistics,
            config,
            user_commands_tx,
        }
    }
}

/// The `/rest/*` routes. Responses are compressed, except audio and images.
pub fn router(state: SubsonicState) -> Router {
    let predicate = DefaultPredicate::new()
        .and(NotForContentType::const_new("audio/"))
        .and(NotForContentType::const_new("application/octet-stream"));
    Router::new()
        .route("/rest/{endpoint}", get(handle).post(handle))
        .layer(CompressionLayer::new().compress_when(predicate))
        .with_state(state)
}

/// Request parameters from the query string and, for a POST, the form body.
/// Repeated names (`id`, `songId`, ...) keep every value.
#[derive(Debug, Default, Clone)]
pub struct Params(Vec<(String, String)>);

impl Params {
    fn parse(query: Option<&str>, form: Option<&[u8]>) -> Self {
        let mut pairs: Vec<(String, String)> = query.and_then(|q| serde_urlencoded::from_str(q).ok()).unwrap_or_default();
        if let Some(body) = form {
            if let Ok(more) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(body) {
                pairs.extend(more);
            }
        }
        Self(pairs)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn all(&self, name: &str) -> Vec<&str> {
        self.0.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect()
    }

    /// # Errors
    /// Code 10 when the parameter is missing.
    pub fn required(&self, name: &str) -> Result<&str, ApiError> {
        self.get(name).ok_or_else(|| ApiError::missing(name))
    }

    /// A non-negative number, `default` when missing or unparsable, at most `max`.
    pub fn count(&self, name: &str, default: usize, max: usize) -> usize {
        self.get(name)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(default)
            .min(max)
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.trim().parse().ok())
    }

    pub fn flag(&self, name: &str, default: bool) -> bool {
        self.get(name).map_or(default, |v| matches!(v.trim(), "true" | "1"))
    }

    fn credentials(&self) -> auth::Credentials<'_> {
        auth::Credentials {
            user: self.get("u"),
            password: self.get("p"),
            token: self.get("t"),
            salt: self.get("s"),
        }
    }
}

async fn handle(
    State(state): State<SubsonicState>,
    AxumPath(endpoint): AxumPath<String>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let form = (method == Method::POST && !body.is_empty()).then_some(body.as_ref());
    let params = Params::parse(query.as_deref(), form);
    let format = Format::from_params(params.get("f"), params.get("callback"));
    let endpoint = endpoint.strip_suffix(".view").unwrap_or(&endpoint).to_string();

    let settings = state.config.get_settings().subsonic_settings;
    if !auth::is_available(&settings, std::env::var_os("DEMO_MODE").is_some()) {
        return response::render(
            &format,
            &Err(ApiError::generic("The Subsonic API is turned off in RSPlayer settings")),
        );
    }
    // OpenSubsonic: answered without authentication.
    if endpoint == "getOpenSubsonicExtensions" {
        return response::render(&format, &Ok(Some(library::open_subsonic_extensions())));
    }
    if let Err(e) = auth::check(&settings, params.credentials()) {
        log::info!(
            "Subsonic: rejected {endpoint} request from client {:?}",
            params.get("c").unwrap_or("")
        );
        return response::render(&format, &Err(e));
    }
    log::debug!("Subsonic: {endpoint} from client {:?}", params.get("c").unwrap_or(""));

    match endpoint.as_str() {
        "stream" | "download" => media::stream(&state, &params, &headers, &format).await,
        "getCoverArt" => media::cover_art(&state, &params, &format).await,
        "startScan" => {
            let result = library::start_scan(&state).await;
            response::render(&format, &result)
        }
        _ => {
            let result = tokio::task::spawn_blocking(move || library::dispatch(&state, &endpoint, &params)).await;
            let result = result.unwrap_or_else(|e| {
                log::error!("Subsonic request failed: {e}");
                Err(ApiError::generic("Internal error"))
            });
            response::render(&format, &result)
        }
    }
}
