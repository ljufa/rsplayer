# Configuration

To configure `rsplayer`, navigate to the settings page in the web UI. Settings are organized into collapsible sections. Here is an overview of the available settings.

## Appearance

-   **Theme:** Select a visual theme for the web UI. Available themes: Dark, Light, Solarized, Dracula, Nord, Rose Pine, Ocean, Gruvbox, Catppuccin, and Hi-Contrast.

## Playback

-   **Audio interface:** Selects the primary audio device for playback. Options include your available ALSA hardware cards, a `Pipewire` virtual card (if `wpctl` is installed on the host), or `Local Browser Playback` for streaming audio directly to your device's web browser.
-   **PCM Device:** Choose the specific PCM device for the selected audio interface (hidden if Local Browser Playback is selected).
-   **Alsa mixer:** When using an ALSA audio interface, selects the specific mixer control used for volume adjustment. Shown on the same row as Volume step.
-   **Volume step:** The amount to increase or decrease the volume with each step.
-   **Auto resume playback on start:** If enabled, `rsplayer` will automatically resume playback of the last track when it starts.

### Advanced

These settings are hidden under the **Advanced** collapsible inside the Playback section. Defaults work well for most setups.

-   **Input buffer size (MB):** The size of the buffer for audio data read from disk or network, in megabytes (1-200). (Hidden if Local Browser Playback is selected).
-   **Ring buffer size (ms):** The size of the ring buffer between the decoder and the ALSA output stream, in milliseconds (1-10000). (Hidden if Local Browser Playback is selected).
-   **Player thread priority:** The real-time priority of the player thread, from 1 to 99. Higher values reduce the risk of audio dropouts on loaded systems. (Hidden if Local Browser Playback is selected).
-   **Fixed output sample rate:** When set, RSPlayer resamples all audio to this rate regardless of the source or device capabilities. Leave at "Auto (recommended)" unless your DAC requires a fixed clock rate.
-   **Set alsa buffer frame size (Experimental!):** Manually override the ALSA hardware buffer frame size. (Hidden if Local Browser Playback is selected).

## Audio Processing

-   **Enable VU meter:** Displays a real-time VU meter on the player page during playback.
-   **Enable loudness normalization (EBU R128):** When enabled, playback volume is automatically adjusted to match a target loudness level using the EBU R128 standard. Loudness analysis runs in the background while playback is stopped — each song is measured once and the result is stored permanently. Progress can be tracked on the Library Statistics page.
-   **Target loudness (LUFS):** Sets the target loudness level for normalization, from -30 to -5 LUFS (default: -18). Only visible when loudness normalization is enabled.

## Music Library

The Music Sources section manages where rsplayer looks for your music files. You can combine multiple local directories and network shares.

Supported file extensions: `.flac`, `.wav`, `.aiff`, `.aif`, `.ape`, `.mp3`, `.mp2`, `.mp1`, `.m4a`, `.ogg`, `.oga`, `.caf`, `.mka`, `.weba`, `.dsf`, `.dff`

### Local Directories

-   **Add Local Directory:** Enter the full path to a music directory and click "Add". The directory is added to the list of music sources.
-   **Remove:** Remove a directory from the music sources. No files are deleted on disk.
-   After adding or removing directories, click **Update library** to scan for new tracks, or **Full rescan** to rebuild the entire library.

### Network Mounts

The Network Mounts section (collapsible) lets you mount remote SMB/CIFS or NFS shares directly from rsplayer.

-   **Add Network Mount:** Provide a name (optional — auto-derived from share path if blank), type (SMB or NFS), server address, and share path. For SMB shares, you can optionally provide a username, password, and Windows domain. Clicking "Mount" creates the mount at `/mnt/rsplayer/<name>` and automatically registers it as a music directory.
-   **Mount/Unmount:** Toggle mounting of saved network shares. Status indicators show whether each share is accessible (Read/Write, Read only, Not mounted, Not accessible).
-   **Remove:** Unmounts (if rsplayer-managed) and removes the share from the saved list.
-   **Detected Network Mounts:** Network filesystems already mounted on the system (e.g., via `/etc/fstab` or manually) are automatically detected and listed. Click "Save" to add them as music sources without re-mounting.

## DSP Settings

The DSP (Digital Signal Processing) section provides a parametric equalizer for fine-tuning audio output. You can add multiple filters, each applied to specific channels or all channels.

Available filter types:
-   **Peaking** — Boost or cut at a specific frequency (parameters: frequency, Q, gain).
-   **Low Shelf / High Shelf** — Boost or cut below/above a frequency (parameters: frequency, Q or slope, gain).
-   **Low Pass / High Pass** — Remove frequencies below/above a cutoff (parameters: frequency, Q).
-   **Band Pass** — Pass only a range of frequencies (parameters: frequency, Q).
-   **Notch** — Remove a narrow band of frequencies (parameters: frequency, Q).
-   **All Pass** — Shift phase without changing amplitude (parameters: frequency, Q).
-   **Linkwitz Transform** — Re-align speaker low-frequency response (parameters: actual freq/Q, target freq/Q).
-   **Gain** — Simple volume adjustment (parameter: gain in dB).

DSP also supports loading built-in presets and importing CamillaDSP configuration files (.yml/.yaml).

?>**Note:** The Alsa mixer and Volume step fields are hidden when the USB firmware link is enabled — volume is managed by the firmware in that case. The control method (ALSA or Pipewire) is set automatically based on the selected audio interface.

## RSPlayer firmware(control board) USB link

-   **Enable link with rsplayer firmware:** Enables communication over a USB serial connection with custom rsplayer firmware control boards.
-   **(When enabled):** Provides quick action buttons to **Power Off** or **Power On** the connected firmware hardware.

## Hardware Integration

For DIY enthusiasts looking to integrate `rsplayer` with custom hardware, all related resources have been moved to dedicated repositories to streamline development and maintenance.

- **Hardware Designs & KiCad Files:** All hardware schematics, PCB layouts (KiCad), and documentation are available in the [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware) repository.
- **Firmware:** The firmware for microcontrollers and other hardware components is located in the [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware) repository.

Please refer to the documentation within these repositories for detailed guides on hardware setup, configuration, and development.
