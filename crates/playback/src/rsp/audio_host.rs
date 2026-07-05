//! Host + output-device selection shared by the local playback path
//! (`symphonia.rs`) and the multiroom follower sink (`sync_sink.rs`).
//!
//! The configured `audio_device` string carries the host choice as a prefix:
//! a value of `asio:<driver name>` selects the Windows ASIO host and the named
//! ASIO driver; any other value uses the platform default host (WASAPI on
//! Windows, ALSA/PipeWire on Linux, `CoreAudio` on macOS).

use anyhow::{Context, Result, format_err};
use cpal::traits::{DeviceTrait, HostTrait};

/// Prefix marking an `audio_device` value as an ASIO driver selection.
pub const ASIO_PREFIX: &str = "asio:";

/// Select the cpal host for an `audio_device` value and return it together with
/// the device key (the string with any host prefix stripped).
#[cfg(all(target_os = "windows", feature = "asio"))]
fn open_host(audio_device: &str) -> Result<(cpal::Host, &str)> {
    if let Some(key) = audio_device.strip_prefix(ASIO_PREFIX) {
        let host = cpal::host_from_id(cpal::HostId::Asio).context("ASIO host is not available")?;
        return Ok((host, key));
    }
    Ok((cpal::default_host(), audio_device))
}

/// Builds without the ASIO backend always use the default host. A stale
/// `asio:` selection has its prefix stripped so device matching still runs
/// against the default host rather than looking for a literal `asio:` name.
#[cfg(not(all(target_os = "windows", feature = "asio")))]
// Signature kept symmetric with the ASIO variant, which genuinely can fail.
#[allow(clippy::unnecessary_wraps)]
fn open_host(audio_device: &str) -> Result<(cpal::Host, &str)> {
    let key = audio_device.strip_prefix(ASIO_PREFIX).unwrap_or(audio_device);
    Ok((cpal::default_host(), key))
}

/// Resolve the configured `audio_device` to a concrete cpal device.
///
/// Returns the device and whether it was opened on the ASIO host (used to pick
/// ASIO-appropriate buffer sizing downstream). An empty or `"default"` key maps
/// to the chosen host's default output device.
pub fn find_device(audio_device: &str) -> Result<(cpal::Device, bool)> {
    let (host, key) = open_host(audio_device)?;
    let is_asio = cfg!(all(target_os = "windows", feature = "asio")) && audio_device.starts_with(ASIO_PREFIX);

    let device = if key.is_empty() || key == "default" {
        host.default_output_device()
            .ok_or_else(|| format_err!("Default audio device not found!"))?
    } else {
        #[allow(deprecated)]
        host.devices()?
            .find(|d| {
                // cpal 0.18: desc.name() is a human-readable label; the backend-specific
                // pcm_id (what alsa.rs stores in config) lives in d.id(). Match id first,
                // fall back to display name for non-ALSA platforms (WASAPI/ASIO/CoreAudio).
                d.id().is_ok_and(|id| id.id() == key) || d.description().is_ok_and(|desc| desc.name() == key)
            })
            .with_context(|| format!("Device {key} not found!"))?
    };

    Ok((device, is_asio))
}
