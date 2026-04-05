# Configuration

To configure `rsplayer`, navigate to the settings page in the web UI. Settings are organized into collapsible sections. Here is an overview of the available settings.

![Settings Overview](/_assets/settings_main.png)

## Appearance

![Appearance Settings](/_assets/settings_appearance.png)

- **Theme:** Select a visual theme for the web UI. Available themes: Dark, Light, Solarized, Dracula, Nord, Rose Pine, Ocean, Gruvbox, Catppuccin, and Hi-Contrast.

## Playback

![Playback Settings](/_assets/settings_playback.png)

- **Audio interface:** Selects the primary audio device for playback. Options include your available ALSA hardware cards, a `Pipewire` virtual card (if `wpctl` is installed on the host), or `Local Browser Playback` for streaming audio directly to your device's web browser.
- **PCM Device:** Choose the specific PCM device for the selected audio interface (hidden if Local Browser Playback is selected).
- **Alsa mixer:** When using an ALSA audio interface, selects the specific mixer control used for volume adjustment. Shown on the same row as Volume step.
- **Volume step:** The amount to increase or decrease the volume with each step.
- **Auto resume playback on start:** If enabled, `rsplayer` will automatically resume playback of the last track when it starts.

### Advanced

These settings are hidden under the **Advanced** collapsible inside the Playback section. Defaults work well for most setups.

- **Input buffer size (MB):** The size of the buffer for audio data read from disk or network, in megabytes (1-200). (Hidden if Local Browser Playback is selected).
- **Ring buffer size (ms):** The size of the ring buffer between the decoder and the ALSA output stream, in milliseconds (1-10000). (Hidden if Local Browser Playback is selected).
- **Player thread priority:** The real-time priority of the player thread, from 1 to 99. Higher values reduce the risk of audio dropouts on loaded systems. (Hidden if Local Browser Playback is selected).
- **Fixed output sample rate:** When set, RSPlayer resamples all audio to this rate regardless of the source or device capabilities. Leave at "Auto (recommended)" unless your DAC requires a fixed clock rate.
- **Set alsa buffer frame size (Experimental!):** Manually override the ALSA hardware buffer frame size. (Hidden if Local Browser Playback is selected).

## Audio Processing

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

Supported file extensions: `.flac`, `.wav`, `.aiff`, `.aif`, `.ape`, `.mp3`, `.mp2`, `.mp1`, `.m4a`, `.ogg`, `.oga`, `.caf`, `.mka`, `.weba`, `.dsf`, `.dff`

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

## Hardware

![Hardware Settings](/_assets/settings_hardware.png)

### RSPlayer Firmware (Control Board) USB Link

- **Enable link with rsplayer firmware:** Enables communication over a USB serial connection with custom rsplayer firmware control boards.
- **(When enabled):** Provides quick action buttons to **Power Off** or **Power On** the connected firmware hardware.

?>**Note:** The Alsa mixer and Volume step fields are hidden when the USB firmware link is enabled — volume is managed by the firmware in that case. The control method (ALSA or Pipewire) is set automatically based on the selected audio interface.

### Hardware Integration

For DIY enthusiasts looking to integrate `rsplayer` with custom hardware, all related resources have been moved to dedicated repositories to streamline development and maintenance.

- **Hardware Designs & KiCad Files:** All hardware schematics, PCB layouts (KiCad), and documentation are available in the [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware) repository.
- **Firmware:** The firmware for microcontrollers and other hardware components is located in the [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware) repository.

Please refer to the documentation within these repositories for detailed guides on hardware setup, configuration, and development.

## System Actions

| Button | Action |
|--------|--------|
| Save (without restart) | Apply settings that don't require restart |
| Save & restart player | Apply settings and restart |
| Restart player | Restart without saving |
| Restart system | Reboot the device |
| Shutdown system | Power off the device |

> **Note:** When DSP or other settings that require restart are changed, RSPlayer will prompt you to restart. If you navigate away from Settings with unapplied changes, a confirmation dialog will warn you.
