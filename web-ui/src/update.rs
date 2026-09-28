//! Checks the GitHub releases API for a newer published version of RSPlayer.
//!
//! The check runs once per app load (after `/api/settings` delivers the running
//! version) directly from the browser — the GitHub API allows cross-origin GET.
//! A dismissed version is remembered in `localStorage` so the banner doesn't
//! reappear on every page load until the next release. The latest release is
//! also cached there for a day, so GitHub is asked at most once per day per
//! browser instead of on every page load.

use api_models::settings::InstallMethod;
use gloo_net::http::Request;
use serde::Deserialize;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/ljufa/rsplayer/releases/latest";
/// Human-facing page the update banner links to.
pub const RELEASES_PAGE: &str = "https://github.com/ljufa/rsplayer/releases/latest";
const DISMISSED_KEY: &str = "rsplayer_update_dismissed";
/// Cached check result, stored as `<unix millis>|<latest version>`.
const LAST_CHECK_KEY: &str = "rsplayer_update_last_check";
const CHECK_INTERVAL_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// Returns the latest released version if it is newer than `current`.
pub async fn check_for_update(current: &str) -> Option<String> {
    let latest = match cached_latest() {
        Some(latest) => latest,
        None => {
            let latest = fetch_latest().await?;
            if let Some(storage) = local_storage() {
                let _ = storage.set_item(LAST_CHECK_KEY, &format!("{}|{latest}", js_sys::Date::now()));
            }
            latest
        }
    };
    if is_newer(current, &latest) {
        Some(latest)
    } else {
        None
    }
}

async fn fetch_latest() -> Option<String> {
    let resp = Request::get(LATEST_RELEASE_API).send().await.ok()?;
    if !resp.ok() {
        return None;
    }
    let release: LatestRelease = resp.json().await.ok()?;
    Some(release.tag_name.trim().trim_start_matches('v').to_string())
}

/// Latest version from a check made less than `CHECK_INTERVAL_MS` ago.
fn cached_latest() -> Option<String> {
    let entry = local_storage()?.get_item(LAST_CHECK_KEY).ok()??;
    let (checked_at, latest) = entry.split_once('|')?;
    let age = js_sys::Date::now() - checked_at.parse::<f64>().ok()?;
    (0.0..CHECK_INTERVAL_MS)
        .contains(&age)
        .then(|| latest.to_string())
        .filter(|l| !l.is_empty())
}

/// True if the user already dismissed the notification banner for `version`.
pub fn is_dismissed(version: &str) -> bool {
    dismissed_version().as_deref() == Some(version)
}

/// Terminal command that applies the update for the given install method,
/// or `None` when there is no command to suggest (desktop app, manual build).
pub const fn update_command(method: InstallMethod) -> Option<&'static str> {
    match method {
        InstallMethod::Rpm => Some("sudo dnf update rsplayer"),
        InstallMethod::Deb => Some("sudo apt update && sudo apt install rsplayer"),
        InstallMethod::Flatpak => Some("flatpak update io.github.ljufa.rsplayer"),
        InstallMethod::Snap => Some("sudo snap refresh rsplayer"),
        InstallMethod::Desktop | InstallMethod::Android | InstallMethod::Unknown => None,
    }
}

/// Remember that the user dismissed the notification for `version`.
pub fn dismiss(version: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(DISMISSED_KEY, version);
    }
}

fn dismissed_version() -> Option<String> {
    local_storage()?.get_item(DISMISSED_KEY).ok()?
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Numeric, segment-wise version comparison: true if `latest` > `current`.
/// Non-numeric segment suffixes (e.g. `-beta`) are ignored.
fn is_newer(current: &str, latest: &str) -> bool {
    let cur = parse_segments(current);
    let lat = parse_segments(latest);
    let len = cur.len().max(lat.len());
    for i in 0..len {
        let c = cur.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l != c {
            return l > c;
        }
    }
    false
}

fn parse_segments(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|seg| {
            let digits: String = seg.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn newer_versions_are_detected() {
        assert!(is_newer("4.6.0", "4.6.1"));
        assert!(is_newer("4.6.0", "4.7.0"));
        assert!(is_newer("4.6.0", "5.0.0"));
        assert!(is_newer("4.6", "4.6.1"));
        assert!(is_newer("4.6.0", "v4.7.0"));
    }

    #[test]
    fn same_or_older_versions_are_ignored() {
        assert!(!is_newer("4.6.0", "4.6.0"));
        assert!(!is_newer("4.6.0", "4.5.9"));
        assert!(!is_newer("4.7.0", "4.6.1"));
        assert!(!is_newer("4.6.0", "4.6"));
        assert!(!is_newer("4.6.0", "not-a-version"));
    }

    #[test]
    fn prerelease_suffixes_compare_numerically() {
        assert!(is_newer("4.6.0", "4.6.1-beta"));
        assert!(!is_newer("4.6.1", "4.6.1-beta"));
    }
}
