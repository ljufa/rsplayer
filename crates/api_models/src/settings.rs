//! The persisted [`Settings`] tree.
//!
//! One JSON blob stored by `rsplayer_config` (fjall) and round-tripped whole
//! through `GET/POST /api/settings` — there is no merge: a client posting an
//! older schema silently drops newer sections, which is why every newer field
//! carries `#[serde(default)]`. Serialized names are frozen (e.g. the
//! `alsa_*` names predate cross-platform support and now configure the cpal
//! output on every OS).

use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};
use validator::Validate;

use crate::common::{AudioCard, CardMixer, PcmOutputDevice, VolumeCrtlType};
use crate::podcast::PodcastDirectory;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Settings {
    pub volume_ctrl_settings: VolumeControlSettings,
    #[validate(nested)]
    pub alsa_settings: AlsaSettings,
    #[serde(default)]
    pub available_volume_control_types: Vec<VolumeCrtlType>,
    #[serde(default)]
    pub network_mounts_available: bool,
    #[serde(default)]
    pub auto_resume_playback: bool,
    pub metadata_settings: MetadataStoreSettings,
    pub playback_queue_settings: PlaybackQueueSetting,
    #[serde(default)]
    pub playlist_settings: PlaylistSetting,
    #[serde(default)]
    #[validate(nested)]
    pub rs_player_settings: RsPlayerSettings,
    #[serde(default)]
    #[validate(nested)]
    pub usb_settings: UsbCmdChannelSettings,
    #[serde(default)]
    #[validate(nested)]
    pub network_storage_settings: NetworkStorageSettings,
    #[serde(default)]
    pub local_browser_playback: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub demo_mode: bool,
    #[serde(default)]
    pub desktop_mode: bool,
    #[serde(default)]
    pub remote_access_url: Option<String>,
    #[serde(default)]
    pub ui_preferences: UiPreferences,
    #[serde(default)]
    #[validate(nested)]
    pub multiroom_settings: MultiroomSettings,
    #[serde(default)]
    pub install_method: InstallMethod,
    #[serde(default)]
    #[validate(nested)]
    pub podcast_settings: PodcastSettings,
    #[serde(default)]
    pub desktop_settings: DesktopSettings,
}

/// How this rsplayer instance was installed — detected by the server at runtime
/// and used by the UI to suggest the matching update command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InstallMethod {
    Flatpak,
    Snap,
    Rpm,
    Deb,
    Desktop,
    /// Android app: updated by the store it was installed from (F-Droid, GitHub APK).
    Android,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MultiroomSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_room_name")]
    #[validate(length(min = 1, max = 64))]
    pub room_name: String,
    /// How far ahead followers buffer audio; also the leader's own output delay while grouped.
    #[serde(default = "default_multiroom_buffer_ms")]
    #[validate(range(min = 200, max = 2000))]
    pub buffer_ms: u32,
    /// Per-device playback trim to compensate constant output-latency differences between rooms.
    #[serde(default)]
    #[validate(range(min = -500, max = 500))]
    pub output_latency_offset_ms: i32,
}

fn default_room_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "RSPlayer".to_string())
}

const fn default_multiroom_buffer_ms() -> u32 {
    750
}

impl Default for MultiroomSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            room_name: default_room_name(),
            buffer_ms: default_multiroom_buffer_ms(),
            output_latency_offset_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PodcastSettings {
    /// Directory used for `PodcastCommand::Search`.
    #[serde(default)]
    pub directory: PodcastDirectory,
    /// Podcast Index credentials (<https://api.podcastindex.org>); only needed
    /// when `directory` is `PodcastIndex`.
    #[serde(default)]
    pub podcast_index_api_key: String,
    #[serde(default)]
    pub podcast_index_api_secret: String,
    /// How often subscribed feeds are re-fetched in the background.
    #[serde(default = "default_podcast_refresh_minutes")]
    #[validate(range(min = 5, max = 1440))]
    pub refresh_interval_minutes: u32,
    /// Newest episodes kept per feed; older ones are pruned unless started.
    #[serde(default = "default_max_episodes_per_feed")]
    #[validate(range(min = 10, max = 5000))]
    pub max_episodes_per_feed: u32,
    /// An episode counts as played once this share of it has been heard.
    #[serde(default = "default_played_threshold_percent")]
    #[validate(range(min = 50, max = 100))]
    pub played_threshold_percent: u8,
}

const fn default_podcast_refresh_minutes() -> u32 {
    60
}

const fn default_max_episodes_per_feed() -> u32 {
    500
}

const fn default_played_threshold_percent() -> u8 {
    95
}

impl Default for PodcastSettings {
    fn default() -> Self {
        Self {
            directory: PodcastDirectory::default(),
            podcast_index_api_key: String::new(),
            podcast_index_api_secret: String::new(),
            refresh_interval_minutes: default_podcast_refresh_minutes(),
            max_episodes_per_feed: default_max_episodes_per_feed(),
            played_threshold_percent: default_played_threshold_percent(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default)]
    pub welcome_shown: bool,
    #[serde(default = "UiPreferences::default_visualizer")]
    pub visualizer: String,
    #[serde(default = "UiPreferences::default_theme")]
    pub theme: String,
    #[serde(default = "UiPreferences::default_show_bg_image")]
    pub show_bg_image: bool,
}

impl UiPreferences {
    fn default_visualizer() -> String {
        "Lissajous".to_string()
    }
    fn default_theme() -> String {
        "dark".to_string()
    }
    const fn default_show_bg_image() -> bool {
        true
    }
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            welcome_shown: false,
            visualizer: Self::default_visualizer(),
            theme: Self::default_theme(),
            show_bg_image: Self::default_show_bg_image(),
        }
    }
}

/// Desktop app window behavior (ignored by the headless server and Android).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSettings {
    /// Closing the window hides it to the system tray and playback goes on;
    /// Quit is in the tray menu. Off: closing the window quits the app.
    #[serde(default = "DesktopSettings::default_close_to_tray")]
    pub close_to_tray: bool,
    /// Linux: replace the OS title bar with the app's nav bar (window
    /// buttons and drag area). Off: the desktop's own title bar.
    #[serde(default = "DesktopSettings::default_custom_titlebar")]
    pub custom_titlebar: bool,
}

impl DesktopSettings {
    const fn default_close_to_tray() -> bool {
        true
    }
    const fn default_custom_titlebar() -> bool {
        true
    }
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            close_to_tray: Self::default_close_to_tray(),
            custom_titlebar: Self::default_custom_titlebar(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NormalizationSource {
    /// Prefer file tags (track gain); fall back to `RSPlayer` EBU R128 calculated loudness.
    #[default]
    Auto,
    /// Use `REPLAYGAIN_TRACK_GAIN` or `R128_TRACK_GAIN` from the file, applied as-is.
    FileTagsTrack,
    /// Use `REPLAYGAIN_ALBUM_GAIN` or `R128_ALBUM_GAIN` from the file, applied as-is.
    FileTagsAlbum,
    /// Use `RSPlayer`'s own EBU R128 integrated loudness measurement (original behavior).
    Calculated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct RsPlayerSettings {
    pub enabled: bool,

    #[serde(default = "input_stream_buffer_size_default_value")]
    #[validate(range(min = 1, max = 200))]
    pub input_stream_buffer_size_mb: usize,

    #[serde(default = "ring_buffer_size_default_value")]
    #[validate(range(min = 100, max = 10000))]
    pub ring_buffer_size_ms: usize,

    #[serde(default = "thread_priority_default_value")]
    #[validate(range(min = 1, max = 99))]
    pub player_threads_priority: u8,
    pub alsa_buffer_size: Option<u32>,
    #[serde(default)]
    pub fixed_output_sample_rate: Option<u32>,
    #[serde(default)]
    #[validate(nested)]
    pub dsp_settings: DspSettings,
    #[serde(default = "default_vu_meter_enabled")]
    pub vu_meter_enabled: bool,
    #[serde(default)]
    pub loudness_normalization_enabled: bool,
    #[serde(default = "default_normalization_target_lufs")]
    pub loudness_normalization_target_lufs: f64,
    #[serde(default)]
    pub loudness_normalization_source: NormalizationSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DspSettings {
    #[serde(default = "default_dsp_enabled")]
    pub enabled: bool,
    #[serde(default)]
    #[validate(nested)]
    pub filters: Vec<FilterConfig>,
}

impl Default for DspSettings {
    fn default() -> Self {
        Self {
            enabled: default_dsp_enabled(),
            filters: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct FilterConfig {
    #[serde(flatten)]
    pub filter: DspFilter,
    #[serde(default)]
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DspFilter {
    Peaking {
        freq: f64,
        q: f64,
        gain: f64,
    },
    LowShelf {
        freq: f64,
        #[serde(default)]
        q: Option<f64>,
        #[serde(default)]
        slope: Option<f64>,
        gain: f64,
    },
    HighShelf {
        freq: f64,
        #[serde(default)]
        q: Option<f64>,
        #[serde(default)]
        slope: Option<f64>,
        gain: f64,
    },
    LowPass {
        freq: f64,
        q: f64,
    },
    HighPass {
        freq: f64,
        q: f64,
    },
    BandPass {
        freq: f64,
        q: f64,
    },
    Notch {
        freq: f64,
        q: f64,
    },
    AllPass {
        freq: f64,
        q: f64,
    },
    LowPassFO {
        freq: f64,
    },
    HighPassFO {
        freq: f64,
    },
    LowShelfFO {
        freq: f64,
        gain: f64,
    },
    HighShelfFO {
        freq: f64,
        gain: f64,
    },
    LinkwitzTransform {
        freq_act: f64,
        q_act: f64,
        freq_target: f64,
        q_target: f64,
    },
    Gain {
        gain: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, EnumIter, EnumString, IntoStaticStr)]
pub enum NetworkMountType {
    #[default]
    Smb,
    Nfs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct NetworkMountConfig {
    #[validate(length(min = 1))]
    pub name: String,
    pub mount_type: NetworkMountType,
    #[validate(length(min = 1))]
    pub server: String,
    #[validate(length(min = 1))]
    pub share: String,
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, Default)]
pub struct NetworkStorageSettings {
    #[serde(default)]
    pub mounts: Vec<NetworkMountConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct UsbCmdChannelSettings {
    pub enabled: bool,
    pub baud_rate: u32,
}

const fn thread_priority_default_value() -> u8 {
    40
}
const fn ring_buffer_size_default_value() -> usize {
    1000
}
const fn input_stream_buffer_size_default_value() -> usize {
    10
}
const fn default_dsp_enabled() -> bool {
    false
}
const fn default_vu_meter_enabled() -> bool {
    true
}
const fn default_normalization_target_lufs() -> f64 {
    -18.0
}

impl Default for RsPlayerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_stream_buffer_size_mb: 10,
            ring_buffer_size_ms: 1000,
            player_threads_priority: 40,
            alsa_buffer_size: None,
            fixed_output_sample_rate: None,
            dsp_settings: DspSettings::default(),
            vu_meter_enabled: default_vu_meter_enabled(),
            loudness_normalization_enabled: false,
            loudness_normalization_target_lufs: default_normalization_target_lufs(),
            loudness_normalization_source: NormalizationSource::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VolumeControlSettings {
    pub volume_step: u8,
    pub ctrl_device: VolumeCrtlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alsa_mixer_name: Option<String>,
    #[serde(skip)]
    pub alsa_mixer: Option<CardMixer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_volume: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_before_mute: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MetadataStoreSettings {
    /// Legacy field kept for backward compat deserialization only.
    #[serde(default, skip_serializing)]
    pub music_directory: String,
    #[serde(default)]
    pub music_directories: Vec<String>,
    pub follow_links: bool,
    #[serde(default, skip_serializing)]
    pub supported_extensions: Vec<String>,
    pub db_path: String,
    /// Libraries read over the network (S3-compatible object storage).
    #[serde(default)]
    pub remote_libraries: Vec<RemoteLibrary>,
}

/// Placeholder sent to the UI instead of stored secrets; posting it back
/// keeps the stored value.
pub const SECRET_MASK: &str = "********";

/// A music library that lives on remote storage instead of a local path.
/// Songs from it are stored under keys `rs3:<id>/<object key>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteLibrary {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub kind: RemoteLibraryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RemoteLibraryKind {
    S3 {
        /// e.g. `https://music.example.com` or `https://s3.eu-central-1.amazonaws.com`
        endpoint: String,
        bucket: String,
        /// Signing region. Empty means detect it on connect (the server's
        /// `x-amz-bucket-region` header), falling back to `us-east-1`.
        #[serde(default = "default_s3_region")]
        region: String,
        /// Only objects under this prefix are part of the library.
        #[serde(default)]
        prefix: String,
        access_key: String,
        secret_key: String,
        /// Path-style URLs (`endpoint/bucket/key`); needed by rclone, `MinIO`, Garage.
        #[serde(default = "default_true")]
        path_style: bool,
    },
}

impl Default for RemoteLibraryKind {
    fn default() -> Self {
        Self::S3 {
            endpoint: String::new(),
            bucket: String::new(),
            region: default_s3_region(),
            prefix: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            path_style: true,
        }
    }
}

impl RemoteLibraryKind {
    pub fn secret(&self) -> &str {
        match self {
            Self::S3 { secret_key, .. } => secret_key,
        }
    }

    pub const fn secret_mut(&mut self) -> &mut String {
        match self {
            Self::S3 { secret_key, .. } => secret_key,
        }
    }
}

/// Host of AWS's global S3 endpoint, used for `s3://bucket/prefix` URLs.
pub const AWS_S3_HOST: &str = "s3.amazonaws.com";

impl RemoteLibraryKind {
    /// Parses the single URL the settings page asks for into an S3 library.
    ///
    /// * `s3://bucket/prefix`: a bucket on AWS (region detected on connect).
    /// * `https://bucket.s3.<region>.amazonaws.com/prefix`: AWS, virtual-host style.
    /// * `https://host[:port]/bucket/prefix`: any other S3 server, path style
    ///   (rclone, `MinIO`, Garage, Cloudflare R2, Backblaze B2, Wasabi...).
    ///
    /// The region comes from the host where providers put it there; otherwise it
    /// stays empty, which means "detect on connect" (see `region`).
    pub fn from_url(url: &str, access_key: &str, secret_key: &str) -> Result<Self, String> {
        let url = url.trim();
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| "URL must start with https://, http:// or s3://".to_string())?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() {
            return Err("URL has no host or bucket".to_string());
        }
        let path = path.trim_matches('/');
        let kind = |endpoint: String, bucket: &str, region: String, prefix: &str, path_style: bool| Self::S3 {
            endpoint,
            bucket: bucket.to_string(),
            region,
            prefix: prefix.to_string(),
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.to_string(),
            path_style,
        };
        match scheme.to_ascii_lowercase().as_str() {
            "s3" => Ok(kind(format!("https://{AWS_S3_HOST}"), authority, String::new(), path, false)),
            "http" | "https" => {
                let host = authority.split(':').next().unwrap_or_default().to_ascii_lowercase();
                if let Some((bucket, region)) = aws_virtual_host(&host) {
                    let endpoint_host = if region.is_empty() {
                        AWS_S3_HOST.to_string()
                    } else {
                        format!("s3.{region}.amazonaws.com")
                    };
                    return Ok(kind(format!("{scheme}://{endpoint_host}"), bucket, region, path, false));
                }
                let (bucket, prefix) = path.split_once('/').unwrap_or((path, ""));
                if bucket.is_empty() {
                    return Err("Add the bucket to the URL: https://host/bucket".to_string());
                }
                Ok(kind(format!("{scheme}://{authority}"), bucket, region_from_host(&host), prefix, true))
            }
            other => Err(format!("Unsupported URL scheme \"{other}\": use https://, http:// or s3://")),
        }
    }

    /// The URL shown for this library; `from_url` parses it back.
    pub fn url(&self) -> String {
        let Self::S3 {
            endpoint,
            bucket,
            prefix,
            path_style,
            ..
        } = self;
        let endpoint = endpoint.trim_end_matches('/');
        let prefix = prefix.trim_matches('/');
        let tail = if prefix.is_empty() { String::new() } else { format!("/{prefix}") };
        if *path_style {
            return format!("{endpoint}/{bucket}{tail}");
        }
        let (scheme, host) = endpoint.split_once("://").unwrap_or(("https", endpoint));
        if host == AWS_S3_HOST || (host.ends_with(".amazonaws.com") && scheme == "https") {
            format!("s3://{bucket}{tail}")
        } else {
            format!("{scheme}://{bucket}.{host}{tail}")
        }
    }

    /// Name used when none is given: bucket and prefix, `/` replaced by `_`
    /// (like network mounts, named after their share).
    pub fn default_name(&self) -> String {
        let Self::S3 { bucket, prefix, .. } = self;
        let prefix = prefix.trim_matches('/');
        let raw = if prefix.is_empty() { bucket.clone() } else { format!("{bucket}/{prefix}") };
        raw.replace('/', "_")
    }
}

/// `bucket.s3.<region>.amazonaws.com`, `bucket.s3-<region>.amazonaws.com` or
/// `bucket.s3.amazonaws.com` -> (bucket, region or "").
fn aws_virtual_host(host: &str) -> Option<(&str, String)> {
    let rest = host.strip_suffix(".amazonaws.com")?;
    let (bucket, service) = rest.split_once(".s3")?;
    if bucket.is_empty() {
        return None;
    }
    let region = service.trim_start_matches(['.', '-']);
    Some((bucket, region.to_string()))
}

/// Region encoded in a provider's path-style endpoint host, "" when unknown.
fn region_from_host(host: &str) -> String {
    if host.ends_with(".r2.cloudflarestorage.com") {
        return "auto".to_string();
    }
    for suffix in [".amazonaws.com", ".backblazeb2.com", ".wasabisys.com"] {
        if let Some(rest) = host.strip_suffix(suffix)
            && let Some(region) = rest.strip_prefix("s3.").or_else(|| rest.strip_prefix("s3-"))
            && !region.is_empty()
            && !region.contains('.')
        {
            return region.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod remote_url_tests {
    use super::*;

    fn parts(kind: &RemoteLibraryKind) -> (&str, &str, &str, &str, bool) {
        let RemoteLibraryKind::S3 {
            endpoint,
            bucket,
            region,
            prefix,
            path_style,
            ..
        } = kind;
        (endpoint, bucket, region, prefix, *path_style)
    }

    fn parse(url: &str) -> RemoteLibraryKind {
        RemoteLibraryKind::from_url(url, " key ", "secret").unwrap()
    }

    #[test]
    fn path_style_server() {
        let k = parse("https://music.ljufa.iz.rs/music");
        assert_eq!(parts(&k), ("https://music.ljufa.iz.rs", "music", "", "", true));
        assert_eq!(k.url(), "https://music.ljufa.iz.rs/music");
        assert_eq!(k.default_name(), "music");
        let RemoteLibraryKind::S3 { access_key, .. } = &k;
        assert_eq!(access_key, "key");
    }

    #[test]
    fn path_style_with_port_and_prefix() {
        let k = parse("http://192.168.5.2:8333/music/Jazz/Live/");
        assert_eq!(parts(&k), ("http://192.168.5.2:8333", "music", "", "Jazz/Live", true));
        assert_eq!(k.url(), "http://192.168.5.2:8333/music/Jazz/Live");
        assert_eq!(k.default_name(), "music_Jazz_Live");
    }

    #[test]
    fn s3_scheme_is_aws_with_detected_region() {
        let k = parse("s3://my-music/lossless");
        assert_eq!(parts(&k), ("https://s3.amazonaws.com", "my-music", "", "lossless", false));
        assert_eq!(k.url(), "s3://my-music/lossless");
    }

    #[test]
    fn aws_virtual_host_urls() {
        let k = parse("https://my-music.s3.eu-central-1.amazonaws.com/lossless");
        assert_eq!(parts(&k), ("https://s3.eu-central-1.amazonaws.com", "my-music", "eu-central-1", "lossless", false));
        assert_eq!(k.url(), "s3://my-music/lossless");
        let k = parse("https://my-music.s3.amazonaws.com");
        assert_eq!(parts(&k), ("https://s3.amazonaws.com", "my-music", "", "", false));
    }

    #[test]
    fn regions_from_provider_hosts() {
        assert_eq!(parts(&parse("https://s3.eu-west-2.amazonaws.com/b")).2, "eu-west-2");
        assert_eq!(parts(&parse("https://s3.eu-central-003.backblazeb2.com/b")).2, "eu-central-003");
        assert_eq!(parts(&parse("https://s3.eu-central-1.wasabisys.com/b")).2, "eu-central-1");
        assert_eq!(parts(&parse("https://abc123.r2.cloudflarestorage.com/b")).2, "auto");
        assert_eq!(parts(&parse("https://minio.example.com/b")).2, "");
    }

    #[test]
    fn invalid_urls() {
        for url in ["music.ljufa.iz.rs/music", "https://music.ljufa.iz.rs", "https://music.ljufa.iz.rs/", "ftp://host/b", "s3://", ""] {
            assert!(RemoteLibraryKind::from_url(url, "k", "s").is_err(), "{url}");
        }
    }

    #[test]
    fn existing_library_round_trips() {
        let stored = RemoteLibraryKind::S3 {
            endpoint: "https://music.ljufa.iz.rs".into(),
            bucket: "music".into(),
            region: "us-east-1".into(),
            prefix: String::new(),
            access_key: "rsplayer".into(),
            secret_key: "x".into(),
            path_style: true,
        };
        assert_eq!(stored.url(), "https://music.ljufa.iz.rs/music");
    }
}

const fn default_true() -> bool {
    true
}

fn default_s3_region() -> String {
    "us-east-1".to_string()
}

impl MetadataStoreSettings {
    /// Replaces stored secrets with [`SECRET_MASK`] before settings leave the server.
    pub fn mask_secrets(&mut self) {
        for lib in &mut self.remote_libraries {
            let secret = lib.kind.secret_mut();
            if !secret.is_empty() {
                *secret = SECRET_MASK.to_string();
            }
        }
    }

    /// Restores secrets the UI sent back masked from `stored`, matched by library id.
    pub fn unmask_secrets(&mut self, stored: &Self) {
        for lib in &mut self.remote_libraries {
            if lib.kind.secret_mut() != SECRET_MASK {
                continue;
            }
            let previous = stored
                .remote_libraries
                .iter()
                .find(|s| s.id == lib.id)
                .map(|s| s.kind.secret().to_string())
                .unwrap_or_default();
            *lib.kind.secret_mut() = previous;
        }
    }

    pub fn find_remote_library(&self, id: &str) -> Option<&RemoteLibrary> {
        self.remote_libraries.iter().find(|l| l.id == id)
    }
}

impl MetadataStoreSettings {
    /// Returns the effective list of music directories.
    /// Falls back to the legacy single `music_directory` if `music_directories` is empty.
    pub fn effective_directories(&self) -> Vec<String> {
        if self.music_directories.is_empty() {
            if self.music_directory.is_empty() {
                vec![]
            } else {
                vec![self.music_directory.clone()]
            }
        } else {
            self.music_directories.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaybackQueueSetting {
    pub db_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct PlaylistSetting {
    pub db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct AlsaSettings {
    #[serde(default)]
    #[validate(nested)]
    pub output_device: PcmOutputDevice,
    #[serde(default)]
    pub available_audio_cards: Vec<AudioCard>,
}
impl AlsaSettings {
    pub fn find_pcms_by_card_id(&self, card_id: &str) -> Vec<PcmOutputDevice> {
        self.available_audio_cards
            .iter()
            .find(|card| card.id == card_id)
            .map(|c| c.pcm_devices.clone())
            .unwrap_or_default()
    }

    pub fn set_output_device(&mut self, card_id: &str, pcm_name: &str) {
        if let Some(pcm) = self.find_pcms_by_card_id(card_id).iter().find(|pcm| pcm.name == pcm_name) {
            self.output_device = pcm.clone();
        }
    }

    pub fn find_mixers_by_card_id(&self, card_id: &str) -> Vec<CardMixer> {
        self.available_audio_cards
            .iter()
            .find(|card| card.id == card_id)
            .map(|mix| mix.mixers.clone())
            .unwrap_or_default()
    }
}

impl Default for MetadataStoreSettings {
    fn default() -> Self {
        Self {
            music_directory: String::new(),
            music_directories: vec![],
            follow_links: true,
            supported_extensions: vec![
                // Lossless
                "flac", "wav", "aiff", "aif", "ape", // Lossy
                "mp3", "mp2", "mp1", "m4a", "ogg", "oga", // Lossless containers
                "caf", // Matroska / WebM (audio-only containers)
                "mka", "weba", // DSD
                "dsf", "dff", // SACD disc image
                "iso",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect(),
            db_path: "ignored_files.db".to_string(),
            remote_libraries: vec![],
        }
    }
}
impl Default for PlaybackQueueSetting {
    fn default() -> Self {
        Self {
            db_path: "queue.db".to_string(),
        }
    }
}
impl Default for PlaylistSetting {
    fn default() -> Self {
        Self {
            db_path: "playlist.db".to_string(),
        }
    }
}

impl Default for UsbCmdChannelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            baud_rate: 115_200,
        }
    }
}

pub const DEFAULT_ALSA_PCM_DEVICE: &str = "hw:0";
pub const DEFAULT_ALSA_MIXER: &str = "0,Master";

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_resume_playback: false,
            volume_ctrl_settings: VolumeControlSettings::default(),
            metadata_settings: MetadataStoreSettings::default(),
            playback_queue_settings: PlaybackQueueSetting::default(),
            alsa_settings: AlsaSettings {
                output_device: PcmOutputDevice::default(),
                available_audio_cards: vec![],
            },
            available_volume_control_types: Vec::new(),
            network_mounts_available: false,
            playlist_settings: PlaylistSetting::default(),
            rs_player_settings: RsPlayerSettings::default(),
            usb_settings: UsbCmdChannelSettings::default(),
            network_storage_settings: NetworkStorageSettings::default(),
            local_browser_playback: false,
            version: String::new(),
            demo_mode: false,
            desktop_mode: false,
            remote_access_url: None,
            ui_preferences: UiPreferences::default(),
            multiroom_settings: MultiroomSettings::default(),
            install_method: InstallMethod::default(),
            podcast_settings: PodcastSettings::default(),
            desktop_settings: DesktopSettings::default(),
        }
    }
}
