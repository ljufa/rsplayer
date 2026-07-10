# Configuration

To configure `rsplayer`, navigate to the settings page in the web UI. Settings are organized into collapsible sections. Here is an overview of the available settings.

?> **First launch:** RSPlayer picks working playback defaults for your platform, so sound works before you ever open Settings. On Windows and macOS it plays through the system default output device with `Software` volume control. On desktop Linux (including the Flatpak and Snap apps) it plays through the `Pipewire` virtual card and controls the system default sink volume directly (falling back to `Software` gain when no `wpctl`/`pactl` is available). On a headless Linux server it plays through the default ALSA device with `Software` volume control. All of this can be changed in Settings at any time — the defaults apply only until you save your own.

![Settings Overview](/_assets/settings_main.png)

## Appearance

![Appearance Settings](/_assets/settings_appearance.png)

- **Theme:** Select a visual theme for the web UI. Available themes: Dark, Light, Synthwave, Dracula, Nord, Dim, Aqua, Coffee, Caramel, and Black.
- **Album art background:** Toggle whether the album artwork is used as the page background.

## Playback

![Playback Settings](/_assets/settings_playback.png)

- **Audio interface:** Selects the primary audio device for playback. The available options depend on your platform:
  - **Linux:** your ALSA hardware cards, a `Pipewire` virtual card (if `wpctl` is installed on the host), or `Local Browser Playback`.
  - **Windows:** the WASAPI output devices (the default host) and, on builds that include the `asio` feature, any installed **ASIO** drivers — shown as `… (ASIO)`. ASIO gives exclusive, low-latency, bit-perfect output; its buffer size and sample rate are set in the driver's own control panel, not in RSPlayer.
  - **macOS:** your CoreAudio output devices.
  - `Local Browser Playback` (all platforms) streams audio directly to your device's web browser instead of playing on the host.
- **PCM output device:** Choose the specific PCM device for the selected audio interface (hidden if Local Browser Playback is selected).

?> **Device list is captured at startup.** RSPlayer enumerates audio devices once when it starts and reuses that list, because probing some backends (notably Windows ASIO, whose drivers are exclusive) while audio is playing can briefly interrupt the output stream. If you connect a new DAC or install a new ASIO driver after launch, restart RSPlayer for it to appear in the list.
- **Auto-resume playback on startup:** If enabled, `rsplayer` will automatically resume playback of the last track when it starts.

### Advanced

These settings are hidden under the **Advanced** collapsible inside the Playback section. Defaults work well for most setups.

- **Input buffer (MB):** The size of the buffer for audio data read from disk or network, in megabytes (1-200). (Hidden if Local Browser Playback is selected).
- **Ring buffer (ms):** The size of the ring buffer between the decoder and the ALSA output stream, in milliseconds (100-10000). (Hidden if Local Browser Playback is selected).
- **Thread priority (1-99):** The real-time priority of the player thread, from 1 to 99. Higher values reduce the risk of audio dropouts on loaded systems. (Hidden if Local Browser Playback is selected).
- **Fixed output sample rate:** When set, RSPlayer resamples all audio to this rate regardless of the source or device capabilities. Leave at "Auto (recommended)" unless your DAC requires a fixed clock rate.
- **ALSA buffer size (frames, 0=default):** Manually override the ALSA hardware buffer frame size. (Hidden if Local Browser Playback is selected).

## Volume Control

![Volume Control Settings](/_assets/settings_volume_control.png)

- **Volume control type:** Select the volume backend used by RSPlayer. Available options are platform-dependent:
  - Linux with ALSA support: `Off`, `Alsa`, `Pipewire`, `Software`
  - Non-ALSA/non-Linux builds: `Off`, `Software`
- **ALSA mixer:** When using an ALSA audio interface, selects the specific mixer control used for volume adjustment.
- **Volume step:** The amount to increase or decrease the volume with each step.

?> **Software volume mode:** In `Software` volume mode, RSPlayer applies a perceptual gain curve in the output path, so volume changes are immediate and independent of hardware mixer support.

## Visualization & Normalization

![Audio Processing Settings](/_assets/settings_audio_processing.png)

- **Enable visualization:** Displays a real-time visualization on the player page during playback. When enabled, a visualizer button appears on the player controls. Press **V** or click the button to cycle through 12 different styles (NeonBar, Spectrum, Wave, Circular, Lissajous, Particles, Mirror, Starfield, DNA, Plasma, Tunnel, Bounce). Your preferred visualizer is saved automatically.
- **Enable loudness normalization (EBU R128):** When enabled, playback volume is automatically adjusted to match a target loudness level using the EBU R128 standard. Loudness analysis runs in the background while playback is stopped — each song is measured once and the result is stored permanently. Progress can be tracked on the Library Statistics page.
- **Normalization source:** Selects where the gain value comes from. Only visible when loudness normalization is enabled.
  - **Auto** *(default)*: Uses track-level gain from file tags if present; falls back to RSPlayer's own EBU R128 calculated loudness. Best choice for mixed libraries.
  - **File tags — track gain**: Reads `REPLAYGAIN_TRACK_GAIN` or `R128_TRACK_GAIN` directly from the file. Works for files already tagged by an external tool (foobar2000, beets, MusicBrainz Picard, etc.).
  - **File tags — album gain**: Reads `REPLAYGAIN_ALBUM_GAIN` or `R128_ALBUM_GAIN` from the file. Useful for preserving intended loudness relationships across an album.
  - **Calculated**: RSPlayer's original behavior — EBU R128 integrated loudness measured in the background and normalized to the configured target LUFS.
- **Target loudness (LUFS):** Sets the target loudness level for normalization, from -30 to -5 LUFS (default: -18). Only visible when the normalization source is set to **Auto** or **Calculated**.

## DSP Settings

The DSP (Digital Signal Processing) section provides a parametric equalizer for fine-tuning audio output. You can add multiple filters, each applied to specific channels or all channels.

Available filter types:

| Filter | Parameters |
|--------|------------|
| Peaking | Freq, Gain, Q |
| Low Shelf | Freq, Gain, Q/Slope |
| High Shelf | Freq, Gain, Q/Slope |
| Low Pass | Freq, Q |
| High Pass | Freq, Q |
| Band Pass | Freq, Q |
| Notch | Freq, Q |
| All Pass | Freq, Q |
| Low Pass FO | Freq |
| High Pass FO | Freq |
| Low Shelf FO | Freq, Gain |
| High Shelf FO | Freq, Gain |
| Gain | Gain |

DSP also supports loading built-in presets and importing CamillaDSP configuration files (.yml/.yaml).

## Music Library

![Music Library Settings](/_assets/settings_music_library.png)

The Music Sources section manages where rsplayer looks for your music files. You can combine multiple local directories and network shares.

Supported file extensions: `.flac`, `.wav`, `.aiff`, `.aif`, `.ape`, `.mp3`, `.mp2`, `.mp1`, `.m4a`, `.ogg`, `.oga`, `.caf`, `.mka`, `.weba`, `.dsf`, `.dff`, `.iso` (SACD disc images)

### Local Directories

- **Add Local Directory:** Enter the full path to a music directory and click "Add". The directory is added to the list of music sources.
- **Remove:** Remove a directory from the music sources. No files are deleted on disk.
- After adding or removing directories, click **Update library** to scan for new tracks, or **Full rescan** to rebuild the entire library.

### Network Mounts

The Network Mounts section (collapsible) lets you mount remote SMB/CIFS or NFS shares directly from rsplayer.

- **Add Network Mount:** Provide a name (optional — auto-derived from share path if blank), type (SMB or NFS), server address, and share path. For SMB shares, you can optionally provide a username, password, and Windows domain. Clicking "Mount" creates the mount at `/mnt/rsplayer/<name>` and automatically registers it as a music directory.
- **Mount/Unmount:** Toggle mounting of saved network shares. Status indicators show whether each share is accessible (Read/Write, Read only, Not mounted, Not accessible).
- **Remove:** Unmounts (if rsplayer-managed) and removes the share from the saved list.
- **Detected Network Mounts:** Network filesystems already mounted on the system (e.g., via `/etc/fstab` or manually) are automatically detected and listed. Click "Save" to add them as music sources without re-mounting.

?> Network mount management is available only on Linux builds. On non-Linux builds the Network Mounts UI is hidden.

## Multiroom

Synchronized playback across multiple RSPlayer devices on the same network. See the dedicated [Multiroom Playback](multiroom.md) page for setup, usage, and how it works.

- **Enable multiroom (synchronized playback):** Turns the feature on and makes this device discoverable by other RSPlayer instances. Requires a restart.
- **Room name:** The name other devices see for this instance (e.g. "Living room"). Defaults to the hostname.
- **Sync buffer (ms):** How far ahead audio is scheduled (default 750). Higher values are more robust against network jitter and slow CPUs; lower values react faster to play/seek. This delays all rooms equally — it does not shift rooms relative to each other.
- **Output latency trim (ms):** Per-room constant offset applied when this device plays as a follower. Positive values delay this room. Only needed for audio drivers that misreport their output latency; leave at 0 otherwise.

## Hardware

![RSPlayer Firmware (Control Board) USB Link](/_assets/settings_hardware.png)

- **Enable USB command channel:** Enables communication over a USB serial connection with custom rsplayer firmware control boards.
- **(When enabled):** Provides quick action buttons to **Power Off** or **Power On** the connected firmware hardware.

?>**Note:** The Alsa mixer and Volume step fields are hidden when the USB firmware link is enabled — volume is managed by the firmware in that case. On Linux, the control method is set automatically based on the selected audio interface; on non-Linux builds firmware integration is not available.

For DIY enthusiasts looking to integrate `rsplayer` with custom hardware, hardware designs and firmware are available in separate repositories:

- **Hardware Designs & KiCad Files:** [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware)
- **Firmware:** [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware)

## System

![System Settings](/_assets/settings_system.png)

| Button | Action |
|--------|--------|
| Restart RSPlayer | Restart the player process without rebooting |
| Restart system | Reboot the device |
| Shutdown system | Power off the device |

The current RSPlayer version is displayed at the bottom of this section.

> **Note:** When DSP or other settings that require restart are changed, RSPlayer will prompt you to restart. If you navigate away from Settings with unapplied changes, a confirmation dialog will warn you.

?> On non-Linux builds, system restart and shutdown actions are not supported and return a UI error notification.
