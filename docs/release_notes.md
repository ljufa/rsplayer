# Release Notes

## v2.9.0 — 2026-04-25

### New Features

#### SACD ISO Playback

RSPlayer can now scan, browse, and play tracks directly from SACD ISO disc images (`.iso` files).

- Both sector encodings are supported: 2048-byte data-only ISOs and 2064-byte physical-sector ISOs. The sector format is auto-detected from the SACDMTOC signature.
- The stereo area is preferred for playback; the system falls back to the first available area if no stereo area is present.
- Only uncompressed DSD (frame_format 2) is supported. DST-compressed discs produce a clear error during scanning.
- Each audio track in the disc is stored as a virtual library entry (`album.iso#SACD_NNNN`), so individual tracks appear in the queue and library like regular files.
- Seeking within a track is supported, aligned to sector boundaries.
- Multichannel (6-channel) disc areas are handled correctly with proper channel layout (FL, FR, FC, LFE, RL, RR).

#### SACD ISO Library Scanning

- `.iso` is now a recognised music extension and is included in library scans.
- Discs that are already scanned (virtual track keys present) are skipped on incremental scans.
- Invalid ISOs (missing SACDMTOC, no valid areas, DST-compressed) log a clear error and are skipped without aborting the scan.


### Removals

#### Ignored Files Database Removed

The `ignored_files.db` database (tracked via `MetadataStoreSettings.db_path`) has been removed. It was originally intended to persist files that failed scanning so they could be skipped on subsequent scans. In practice it was never read — scan failures were always just logged and skipped. The field remains in the settings struct for backward-compatible deserialization of existing configs, but no database file is created or consulted at runtime.

### Bug Fixes

#### Metadata Scan Crash — fjall Lock Poisoning ([#8](https://github.com/ljufa/rsplayer/issues/8))

During a library scan, multiple rayon worker threads were writing to the fjall database concurrently. If any thread encountered a fjall error (I/O, disk pressure), the `.expect()` call in `song_repository.save()` or `album_repository.update_from_song()` would panic. A panic inside a rayon thread while fjall held its internal journal mutex poisoned that lock, causing all other scanning threads to cascade-fail with `PoisonError` and abort the scan.

Two changes were made to fix this:

- `save()` and `update_from_song()` now return `Result<()>` instead of calling `.expect()`. Errors are propagated up to `add_songs_to_db`, where they are logged and the scan continues with the remaining files.
- Metadata scanning is now sequential (plain `for` loop) instead of parallel (`rayon::par_iter`). This eliminates the lock-poisoning failure mode entirely and also fixes a secondary race condition in `update_from_song` where two threads processing tracks from the same album could overwrite each other's `song_keys` updates, silently dropping tracks from the album index.

### Build

#### Release WASM Build Fix

`dx build --release` was failing with a wasm-opt SIGABRT due to DWARF debug info embedded in the WASM binary by default. Added `--debug-symbols false` to the release build task in `Makefile.toml` to suppress DWARF output and allow wasm-opt to complete successfully.

---

## v2.8.0 — 2026-04-21

### Frontend: Rewritten in Dioxus

The web UI has been fully rewritten from the [Seed](https://seed-rs.org/) framework to [Dioxus 0.7](https://dioxuslabs.com/). Seed required an Elm-like message-passing architecture that was verbose and hard to extend; Dioxus uses a React-style component model with fine-grained reactive signals, making the code substantially more direct. The rewrite also served as a complete visual redesign — the old Bulma + FontAwesome stylesheet has been replaced with Tailwind CSS + DaisyUI + Material Icons.

#### Architecture

- **Reactive signals**: Global application state (`AppState`) is a flat struct of `Signal<T>` values, provided via context and subscribed to at the component level. Components re-render only when the signals they read change.
- **SPA routing**: Navigation uses the browser History API (`pushState`) and a `popstate` listener. A `CurrentPath` context signal holds the active route; the root `App` component pattern-matches on it to render the correct page.
- **WebSocket hook**: A dedicated `use_websocket` hook manages the connection lifecycle — connects on mount, dispatches incoming `StateChangeEvent` messages directly into `AppState` signals, and auto-reconnects with a 3-second delay on close or error.
- **Panic overlay**: Unhandled WASM panics now show a full-screen recovery overlay instead of a blank page, with an error details section and buttons to go home or reload.

#### Visual Redesign

- **Tailwind CSS + DaisyUI**: Replaces the Bulma CSS framework. All theming is driven by DaisyUI CSS variables applied to the `<html data-theme>` attribute, enabling consistent dark/light themes across every component.
- **Material Icons (woff2 only)**: FontAwesome has been removed. The icon set is now Material Icons served from a single self-hosted woff2 file, reducing static asset size significantly.
- **Immersive background**: The current album art is displayed as a blurred, blended full-page background behind all content. Toggled per-user preference (persisted in `localStorage`).
- **Footer player bar**: Playback controls live in a persistent footer bar visible on every page, so transport controls are always accessible without navigating to the player page.
- **Skeleton loading states**: All list and tree views show animated skeleton placeholders while data is being fetched over WebSocket, replacing blank flashes.

#### Player Page

- Album art, song title, artist, and album displayed prominently with adaptive font sizing.
- Progress bar with seek support.
- Volume control, mute, playback mode cycle, like/unlike, lyrics toggle, and visualizer toggle all on one screen.
- Gain/LUFS info line shown below the progress bar when loudness normalization is active.

#### Queue Page

- Paginated queue list with drag-and-drop reordering (HTML5 drag events).
- Per-item actions: Play, Play Next, Remove.
- Search bar with clear button; "Focus current song" button jumps to the playing track's page.
- Save queue as playlist and add URL to queue actions in the toolbar.
- Clear queue with confirmation modal.
- Load More button for paginated queues.

#### Library Pages

All library pages are new or substantially rewritten:

- **Artists**: Collapsible tree — Artist → Album → Song. Lazy-loads children on expand. Per-node queue actions: Load, Add, Play Next (songs only).
- **Files**: Directory tree with recursive lazy loading. Search switches to a flat results view. Per-node queue actions: Load (directories), Add (directories), Add / Play Next (files).
- **Playlists**: Horizontally scrollable album/playlist carousels organized into collapsible sections — Recently Added, New Releases, Saved, Favorites, By Genre, By Decade. Section headers have Load All / Add All buttons. Clicking a card opens a modal with the full track list.
- **Radio**: Filter tabs (Favorites, Top, Country, Language, Search). Per-station Add and Play Now actions. Favorite/unfavorite toggle. Browse hierarchy (country → language → station).
- **Stats**: Library summary cards, playback statistics, loudness analysis progress, and bar charts for top genres and albums by decade — all unchanged in functionality but restyled.

#### Modals

- **Welcome / first-time setup**: Shown on first visit; guides to Settings to configure a playback device.
- **Keyboard shortcuts**: Full reference overlay, toggled with `?`.
- **Playlist / album detail**: Opens from any album or playlist card; shows the full track list with pagination.
- **Add URL to queue**: Text input modal for adding a stream URL directly to the queue.
- **Save queue as playlist**: Names and saves the current queue as a static playlist.
- **Clear queue confirmation**: Prevents accidental queue wipe.

#### Notifications

Toast notifications for success and error events from the backend are displayed in the bottom-right corner and auto-dismiss after a few seconds.

---

## v2.7.5 — 2026-04-09

### Bug Fixes

#### Network Volumes Not Mounted on Startup
Managed network shares with a custom mount point path were silently skipped at startup. The auto-mount logic treated any share with an explicit `mount_point` as externally managed (e.g. via `/etc/fstab`) and never attempted to mount it. The check now inspects whether the share is actually mounted rather than whether the path is customized — shares that are not yet mounted are always attempted, regardless of how the mount point is configured.

#### Loudness Analysis Thread Spinning on Unavailable Files
When the music volume was unmounted (or otherwise unreachable), the loudness scan background thread entered a busy loop: it collected all un-analysed songs, found none of the files on disk, skipped them all without recording anything, and immediately started the next pass. This repeated indefinitely at high CPU usage. The thread now detects a pass in which no files could be read and backs off for 60 seconds before retrying.

#### Genre List — Action Buttons Not Vertically Centered
The play and add buttons on the right side of each genre/decade row were not vertically centered within the row. The `<a>` elements were inline by default, so the parent flex container's `align-items: center` had no effect on the icon alignment inside them. Fixed by making both button elements `display: flex` with `align-items: center`.

### Improvements

#### Dependency Upgrades
All workspace and web UI dependencies updated to their latest versions.

#### Error Handling
Replaced `unwrap()` and `expect()` calls across the codebase with proper error propagation and logging, reducing the risk of unexpected panics in edge cases.

#### Code Quality
Clippy and redundant `.clone()` cleanup pass across multiple crates.

---

## v2.7.0 — 2026-04-05

### New Features

#### Loudness Normalization Source Selection
The loudness normalization system now supports multiple gain sources, selectable from Settings → Audio Processing:

- **Auto** *(default)*: Uses track-level gain from file tags if present; falls back to RSPlayer's own EBU R128 calculated loudness. Best choice for mixed libraries.
- **File tags — track gain**: Reads `REPLAYGAIN_TRACK_GAIN` or `R128_TRACK_GAIN` directly from the file and applies it as-is. Works for files already tagged by an external tool (foobar2000, beets, MusicBrainz Picard, etc.).
- **File tags — album gain**: Reads `REPLAYGAIN_ALBUM_GAIN` or `R128_ALBUM_GAIN` from the file. Useful for preserving intended loudness relationships across an album.
- **Calculated**: RSPlayer's original behavior — EBU R128 integrated loudness measured in the background and normalized to the configured target LUFS.

Both ReplayGain (text, e.g. `+2.35 dB`) and EBU R128 (Q7.8 fixed-point integer, e.g. `256`) tag formats are supported with case-insensitive key lookup. When both are present in a file, R128 takes priority over ReplayGain.

The **Target loudness (LUFS)** field is now only shown when the selected source actually uses it (Auto or Calculated modes).

The player info line on the player page now shows the applied gain even when no RSPlayer-measured LUFS is available (e.g. `+1.0 dB (file tag)`), and displays the full chain when both are known: `−14.2 LUFS  →  +1.8 dB  →  −12.4 LUFS`.

#### Demo Mode
A new `demo_mode` setting disables destructive system commands (power off, system restart) and shows a banner across the top of the UI informing the user that some features are not available. Intended for public or shared deployments. Enable by setting `"demo_mode": true` in the settings file or via the `DEMO_MODE` environment variable.

### Bug Fixes

#### Mute — Volume Not Restored After Unmute
Pressing mute and then unmute (via the button or the **M** keyboard shortcut) now restores the volume to exactly the level it was at before muting. Previously, unmuting always set the volume to 50% regardless of the prior level.

The mute button click and the keyboard shortcut now share the same code path, eliminating the subtle divergence that existed between the two.

### Improvements

#### Volume Control Icons
The volume control row now uses visually distinct icons for each button:
- **Mute** button: speaker icon (sound on) / speaker-with-X (muted) — no longer the same as the volume-up button
- **Volume down**: ⊖ (`fa-circle-minus`)
- **Volume up**: ⊕ (`fa-circle-plus`)

#### Settings Page: Playback Section State Across Reloads
When switching between **Local Browser Playback** and a hardware ALSA device, the settings page now saves a flag to `localStorage` before reloading, so the Playback section automatically reopens in the correct state after the forced page reload.

#### Metadata Extractor: ReplayGain and R128 Tag Preservation
The `AudioMetadataExtractor` now stores `REPLAYGAIN_TRACK_GAIN`, `REPLAYGAIN_ALBUM_GAIN`, `REPLAYGAIN_TRACK_PEAK`, and `REPLAYGAIN_ALBUM_PEAK` tags in the song's raw tag map during metadata scanning. This makes them available at playback time without re-reading the file.

#### Internal: Codec Registry and Probe Moved to Crate Root
`build_probe()` and `build_codec_registry()` have been moved from `rsplayer_metadata::dsd_bundle` to the `rsplayer_metadata` crate root. Both functions register all default Symphonia formats and codecs, plus the custom DSD (DSF) and APE readers/decoders. Callers no longer need to import from the internal `dsd_bundle` sub-module.

---

## v2.6.5 — 2026-03-29

### New Features

#### Enhanced Music Visualizer
The music visualizer has been completely redesigned with 12 new visualizer modes:
- **NeonBar**: Classic vertical bars with neon glow effect
- **Spectrum**: Frequency spectrum analyzer
- **Wave**: Oscilloscope-style waveform display
- **Circular**: Radial spectrum display
- **Lissajous**: Parametric curve visualization
- **Particles**: Dynamic particle system
- **Mirror**: Mirrored spectrum display
- **Starfield**: 3D starfield effect
- **DNA**: Double helix visualization
- **Plasma**: Fluid plasma effect
- **Tunnel**: Perspective tunnel effect
- **Bounce**: Bouncing ball visualization

- Toggle visualizer on/off from the player controls (shown when visualization is enabled)
- Press **V** to cycle through visualizer types
- Visualizer style is persisted across sessions
- Visualizer now renders as an immersive background layer behind the player

#### First-Time Setup Wizard
New guided setup for first-time users:
- Welcome modal now includes a "Required Setup" notice
- On first visit, if no playback device is configured, automatically navigates to Settings
- Audio interface dropdown is automatically focused for easy configuration
- Detects both ALSA hardware devices and Local Browser Playback mode

#### System Power Controls
New power management options in Settings:
- **Restart System**: Reboots the entire device
- **Shutdown System**: Powers off the device

### Improvements

#### APE Playback Performance
APE (Monkey's Audio) files are now played directly from disk instead of loading the entire file into memory:
- Only headers and seek table (~1KB) are read upfront; frame data is read on demand during playback
- 1 MB read buffer amortizes NFS/network round trips, preventing audio buffer starvation on slow links
- Removed background prefetch thread for simpler, more reliable decoding
- Added version check (APE format version ≥ 3990 required)

#### APE Metadata Scanning Performance
APE file metadata scanning is now significantly faster:
- Tags are read directly from disk without loading the entire audio file
- APEv2 and ID3v2 tags are extracted using the native `ape_decoder` crate
- Large APE libraries now scan in seconds instead of minutes

#### Settings Page
- **Unsaved changes warning**: When navigating away from Settings with unapplied DSP changes, a confirmation dialog now asks before discarding changes
- **Separate save and restart**: Settings are now saved in two modes:
  - Save without restart (for settings that take effect immediately like theme)
  - Save with restart (for settings that require player restart like audio device)
- DSP dirty state tracking to prevent accidental loss of EQ changes

#### Volume Persistence
- Volume changes via hardware buttons (USB volume up/down) are now persisted immediately
- Firmware-reported volume from USB devices is properly saved and broadcast

#### Database Reliability
- Database is now persisted (WAL sync) after metadata scan completes
- Database is also persisted on graceful shutdown (SIGTERM, SIGINT, Ctrl+C)

#### UI/UX
- All player control buttons now have keyboard shortcut tooltips (e.g., "Play (Space)", "Next (→)")
- Removed redundant keyboard shortcuts hint box from player page
- Track info and controls now render above the visualizer background with proper z-indexing

---

## v2.6.0 — 2026-03-27

### New Features

#### Monkey's Audio (APE) Support
RSPlayer now supports playback of Monkey's Audio (APE) files, a popular lossless audio compression format. The implementation includes a custom Symphonia decoder and demuxer with:
- Full support for 8, 16, 24, and 32-bit sample depths
- Background frame prefetching for smooth playback
- APEv2 and ID3v2 tag reading with proper metadata mapping

#### Global Keyboard Shortcuts
A comprehensive keyboard shortcut system has been added for power users:
- **Space**: Play / Pause
- **← / →**: Previous / Next track (Shift+Arrow for ±10s seek)
- **↑ / ↓**: Volume up / down
- **M**: Mute / Unmute
- **L**: Like / Unlike current track
- **Y**: Toggle lyrics modal
- **S**: Cycle shuffle/repeat mode
- **/**: Focus search input
- **?**: Show keyboard shortcuts help
- **1-4**: Navigate to Player, Queue, Library, Settings
- **F / A / P / R / T**: Navigate to Files, Artists, Playlists, Radio, Stats

#### Breadcrumb Navigation
All library pages now feature breadcrumb navigation at the top, showing the current location within the library hierarchy. This provides clear context and quick navigation back to parent sections.

#### Skeleton Loading Screens
Loading states are now visualized with skeleton screens instead of blank spaces. This provides immediate feedback that content is loading and improves perceived performance.

#### Empty State UI
All pages now display helpful empty state screens when there's no content:
- Queue: "Queue is empty" with call-to-action to browse library
- Playlists: "No playlists yet" with guidance
- Artists/Files: "No results" for empty searches
- Radio: Contextual empty states for filters

### Improvements

#### Radio Stream Metadata
Radio stream metadata extraction has been significantly improved:
- **Radiosphere provider**: Extracts track info, cover art, and channel descriptions
- **Quantumcast provider**: Parses StreamABC metadata for track and cover art
- **ICY metadata reader**: New dedicated ICY metadata parser for Shoutcast/Icecast streams

#### Genre Normalization
A new comprehensive genre utilities module provides:
- Full ID3v1 genre code translation (148 standard genres)
- Case normalization and whitespace handling
- Junk genre filtering

#### Code Architecture
Major refactoring for improved maintainability:
- Backend commands split into dedicated modules (player, queue, playlist, metadata, storage, system)
- New `AudioMetadataExtractor` for centralized metadata extraction
- ALSA output error threshold increased to 30 for better resilience on resource-constrained hardware

### Bug Fixes

- Fixed ALSA output error handling to be more resilient on hardware like Raspberry Pi Zero
- Fixed various UI layout issues with modal backgrounds and button styling

---

## v2.5.5 — 2026-03-25

### New Features

#### Fixed Output Sample Rate
A new **Fixed Output Sample Rate** setting is available under the Playback → Advanced section of the settings page. When set, RSPlayer will always resample audio to the specified rate regardless of the source file's sample rate or what the device reports as supported. Useful for hardware that requires a fixed clock rate (e.g. external DACs locked to 192kHz or 48kHz) or for forcing a known-good rate on devices with unreliable capability reporting.

#### Volume Persistence Across Restarts
The volume level is now saved whenever it changes and automatically restored when the service restarts. Previously, the player always started at the hardware default (maximum) on restart, which could cause a loud shock on powered systems. The volume is now restored to the last-used level, defaulting to 0 on first use.

#### Expanded Audio Format Support
The following file formats are now recognized and scanned into the library:

| Format | Extensions |
|--------|------------|
| AIFF | `.aiff`, `.aif` |
| MPEG Audio (Layer I/II) | `.mp1`, `.mp2` |
| Ogg Audio | `.oga` |
| WebM Audio | `.weba` |

Previously supported formats (FLAC, WAV, MP3, M4A, OGG, DSF, DFF, CAF, MKA) are unchanged.

### Improvements

#### ALSA Driver Compatibility: Automatic Rate Fallback
Some ALSA drivers (e.g. Merus MA12070P) advertise a continuous sample rate range (e.g. 44100–192000 Hz) but only accept specific discrete rates at stream-open time. RSPlayer now detects this condition and automatically probes candidate rates in priority order — integer multiples of the source rate first, then range boundaries, then standard rates — retrying the stream open until one succeeds. This prevents playback failures on hardware with non-compliant ALSA drivers.

#### SMB Network Mount: Domain Support and Improved Credentials Handling
- **Domain field**: SMB shares can now be configured with a Windows domain for environments that require domain-qualified authentication.
- **Inline credentials**: Credentials are now passed directly to the kernel mount call instead of being written to a credentials file (`/opt/rsplayer/creds_*`). This removes the dependency on the credentials file path and simplifies the mount lifecycle.
- **Auto-name from share**: If no mount name is provided in the UI, the name is automatically derived from the share path.
- **Password masking in logs**: Mount log lines no longer expose the password in plaintext.

#### Settings Page: Collapsible Sections and Reorganized Playback Controls
The settings page is now organized into collapsible sections — **Appearance**, **Playback**, **Audio Processing**, and **Music Library** — making it easier to navigate on both desktop and mobile.

The **Playback** section has been further reorganized:
- **Alsa mixer** and **Volume step** are now shown on the same row, directly below the audio interface selector.
- **Input buffer size**, **Ring buffer size**, and **Player thread priority** have been moved into the **Advanced** subsection (collapsed by default), alongside Fixed output sample rate and ALSA buffer frame size.

#### Theme Consistency
Settings page backgrounds now use CSS variables instead of hardcoded colors, ensuring all themes apply correctly across the full UI.

### Upcoming

- **Symphonia 0.6 alpha:** RSPlayer currently uses a custom Symphonia fork (`break_infinite_loop` branch) as a workaround for issues with radio stream handling and API shape differences. An upgrade to the official Symphonia 0.6 alpha is planned once it stabilizes on crates.io.

### Bug Fixes

- **Frontend cache busting**: `package.js` and `package_bg.wasm` (the WASM bundle) were not receiving version query strings on cache-busting. Fixed — both files are now requested with `?v=<version>` so browser caches are correctly invalidated on upgrade.
- **SMB credentials file left behind**: The credentials file (`/opt/rsplayer/creds_<name>`) was not removed on unmount in some cases. This code path has been removed; credentials are no longer written to disk.

---

## v2.5.1 — 2026-03-22

### Improvements

#### Reliable Network Share Auto-Mount on Startup
Managed network shares (SMB/CIFS, NFS) are now mounted automatically when RSPlayer starts, with two reliability improvements:

- **Retry logic**: If a mount fails at startup (e.g. the server isn't yet reachable), RSPlayer retries up to 3 times with a 5-second delay between attempts before giving up and logging a warning.
- **SMB credential persistence**: SMB credentials are now written to a credentials file (`/opt/rsplayer/creds_<name>`) at mount time. On restart, the credentials file is used directly so shares with passwords remount without requiring the password to be stored in the settings file.

#### Systemd Service: Wait for Network Online
The systemd unit now declares `After=network-online.target` (previously `network.target`), ensuring RSPlayer starts only after the network stack is fully ready. This prevents auto-mount failures on systems where shares are reachable only once routes and DNS are configured.

> **Upgrade note:** The updated systemd unit is included in the `.deb`, `.rpm`, and `.tgz` packages. If you installed manually, replace `/etc/systemd/system/rsplayer.service` and run `sudo systemctl daemon-reload`.

---

## v2.5.0 — 2026-03-21

### New Features

#### Network Storage Management
RSPlayer can now discover and manage network mounts (SMB/CIFS and NFS) directly from the settings page.

- **Add Network Mounts**: Configure SMB or NFS shares with server, share, and optional credentials. Mounts are created under `/mnt/rsplayer/<name>` and automatically added as music directories.
- **Discover Existing Mounts**: Network filesystems already mounted on the system (via `/proc/mounts` and `/etc/fstab`) are automatically detected and shown in a separate list. Click "Save" to add them as music sources without re-mounting.
- **Mount/Unmount Controls**: Mount and unmount saved shares directly from the UI with real-time status indicators (Read/Write, Read only, Not mounted, Not accessible).
- **Multiple Music Directories**: The music library now supports multiple source directories. Add local paths or network mounts — all are scanned together.

#### Frontend Cache Busting
Static assets (CSS, JS, WASM) are now served with version-tagged query strings (e.g. `?v=2.5.0`). Browser caches are automatically invalidated on every release — no more stale UI after upgrading. The `index.html` is served with `Cache-Control: no-cache, must-revalidate` to ensure the browser always fetches the latest asset references.

> **Upgrade note:** When upgrading from a version prior to 2.5.0, you may need to hard-refresh your browser (`Ctrl+Shift+R` / `Cmd+Shift+R`) once to clear the previously cached `index.html`. This is a one-time step — future upgrades will invalidate caches automatically.

#### Version Display
The current RSPlayer version is now shown at the bottom of the settings page.

### Improvements

- **Music source hot-reload**: Newly added music directories are now immediately available for playback after scanning, without requiring a player restart.
- **Random playback**: The shuffle algorithm now properly excludes the currently playing song when picking the next track, preventing back-to-back repeats. The current song is also correctly marked as "played" from the start of each shuffle cycle.
- **Queue load actions**: Loading a directory, album, artist, playlist, or song into the queue now always starts playback from the beginning of the first track, instead of incorrectly resuming from the previous song's progress position.

### Bug Fixes

- Fixed metadata scanner using stale settings after configuration changes — scanner now reads fresh settings before each scan.
- Fixed AAC radio stream playback.
- Fixed random playback state not being properly restored after player restart.

---

## v2.1.0 — 2026-03-20

### New Features

#### Automatic Sample Rate Resampling
RSPlayer now automatically resamples audio when the output device doesn't support the source file's sample rate. Previously, playback would fail with an ALSA `snd_pcm_hw_params` error on devices with limited rate support (e.g. TAS58xx I2S DACs that only support 48kHz).

- High-quality FFT-based resampling via [rubato](https://github.com/HEnquist/rubato)
- Automatically detects the closest supported device rate — no user configuration needed
- DSP equalizer and VU meter operate at the output rate for correct filter coefficients and metering
- Zero overhead when no resampling is needed — the existing direct playback path is unchanged
- Set `RSPLAYER_RESAMPLE_TO=<rate>` environment variable to force resampling for testing or debugging

### Improvements

#### Audio Device Selection
- `hw:` (direct hardware) devices are now sorted first in the PCM device dropdown and labeled **(hw, recommended)** to guide users toward the most reliable option
- All device types (`plughw:`, `default`, etc.) remain available for advanced users
- Removed the non-functional `plughw:` fallback path — cpal cannot enumerate ALSA plugin devices, so the fallback could never find the target device

### Bug Fixes

- Fixed VU meter peak calculation for stereo sources

---

## v2.0.0 — 2026-03-17

### Breaking Changes

#### Database Engine: sled replaced with fjall
The embedded database engine has been migrated from **sled** to **fjall**. This is a breaking change — existing sled databases (`metadata.db`, `queue.db`, `loudness.db`, `play_stats.db`) are not compatible and will be recreated on first launch. A full metadata rescan will be required after upgrading.

**Why**: sled has been unmaintained and has known issues with data corruption and write amplification. fjall is an actively maintained LSM-tree storage engine with better write performance and reliability.

### New Features & Improvements

#### RISC-V 64-bit Support
rsplayer now builds and runs on `riscv64gc-unknown-linux-gnu`, expanding support to RISC-V single-board computers and development boards.

#### ALSA as a Feature Flag
ALSA support is now behind a compile-time feature flag (`alsa`, enabled by default). This allows building rsplayer on platforms where ALSA is not available (e.g., RISC-V) without pulling in ALSA dependencies.

#### SSL Made Optional
TLS/SSL for the web server is now optional, simplifying deployment behind a reverse proxy or on local networks where HTTPS is not needed.

#### Confirmation Modals
Destructive actions in the UI now require explicit confirmation:
- **Queue**: Clearing the queue shows a confirmation dialog before deleting all items.
- **Settings**: Restarting the player, saving settings with restart, and triggering a full metadata rescan all prompt for confirmation, preventing accidental data loss or unintended restarts.

### Bug Fixes

- **Random playback mode**: Fixed a bug where random/shuffle mode could select incorrect tracks under certain queue states.

### Platform Support

- Added RISC-V 64-bit (`riscv64gc`) build target in CI/CD pipeline and Cross.toml configuration.
- Fedora, Arch Linux, and Debian packaging continue to be supported.

---

## v1.9.5 — 2026-03-12

### New Features & Improvements

#### EBU R128 Loudness Normalization
Per-song loudness normalization based on the EBU R128 integrated loudness standard is now available.

- **Toggleable in Settings**: Enable/disable normalization and configure the target loudness level (LUFS) on the Settings page under the new loudness normalization section.
- **Background Analysis**: Each song is measured once using the EBU R128 standard. Analysis runs automatically in a dedicated background thread pool (capped at half the available CPU cores) only while playback is stopped, so it never competes with audio output.
- **Persistent Storage**: Loudness values are stored in a dedicated sled database (`loudness.db`) separately from the main metadata store. Results survive restarts and rescans — each file is only measured once.
- **Gain Applied via DSP**: At playback time, the per-song gain is applied through the existing CamillaDSP pipeline, composing correctly with any user EQ filters.

#### Library Statistics Page
A new **Library Statistics** page is available at `#/library/stats` (accessible from the library navigation menu via the bar chart icon).

- **Library summary**: Total songs, albums, artists, and cumulative library duration.
- **Playback stats**: Total play count, unique songs played, and liked songs count.
- **Loudness analysis progress**: Shows how many songs have been analysed with a progress bar and percentage.
- **Top Genres**: Horizontal bar chart of the most represented genres in your library.
- **Albums by Decade**: Horizontal bar chart of album distribution across decades.

#### UI Readability
- **Semi-transparent dark backdrop** added to the Settings and Library Statistics pages so all labels, headings, and text remain legible when an album cover is displayed in the background behind them.
- **Adaptive track title font size** on the player page: shorter titles use a larger font (`is-1`) and long titles scale down (`is-2` / `is-3`) to avoid overflow.

---

## v1.9.0 — 2026-03-08

### New Features & Improvements

- **Library Playlists Page Revamp**: 
  - **Categorized Playlists**: The Static Playlists page now automatically groups your library into categorized carousels by "Genre" and "Decade".
  - **Collapsible Lazy-Loaded Sections**: To improve page load performance and reduce memory usage on large libraries, all playlist categories are now organized into collapsible sections. The core sections (Recently Added, New Releases, Favorites, Saved) remain expanded by default, while Genre and Decade sections display the total album count but remain collapsed. The album data is only fetched and the carousel attached when the user explicitly expands the section, saving significant initial bandwidth.
  - **Genre Normalization and Sanitization**: To avoid fragmenting the library with near-duplicate genres, a comprehensive genre normalization step has been added:
    - Consolidates differing cases (e.g., "electronic", "Electronic", "ELECTRONIC").
    - Translates numeric ID3v1 genre codes (e.g., "(17)", "(20)") into proper display names ("Rock", "Alternative").
    - Automatically filters out junk/unclassified metadata (e.g., "Other", "Unknown genre", "misc").
    - Applies title casing to ensure a clean, consistent display.
  - **Decade Sanitization**: Decades are now strictly validated (only valid 4-digit years >= 1950 are shown) to avoid cluttering the view with malformed or ancient release date data.

### Platform Support (From v1.8.x)
- **Fedora and Arch Linux**: Native package building, installation scripts, and platform support have been expanded to include Fedora (RPM) and Arch Linux.

---

## v1.8.6 — 2026-03-04

### Bug Fixes

- **Queue — "Play Now" on directory causes high CPU/memory and queue.db bloat**: When clicking
  "Play Now" on a high-level directory in the file tree, the queue service was performing a full
  scan of the song database (`get_all_iterator()`) and deserializing every song into memory before
  filtering. For large libraries this spiked RAM and CPU. In addition, each song was added to the
  priority queue via a separate sled read-modify-write cycle, producing O(N²) write amplification
  that caused `queue.db` to grow to hundreds of megabytes. Fixed by:
  - Replacing the full-scan + in-memory filter with `sled::scan_prefix` (new
    `SongRepository::find_songs_by_dir_prefix`), which reads only the matching key range.
  - Batching all priority-queue updates into a single read-modify-write regardless of how many
    songs are added (`add_songs_after_current` in `queue_service.rs`).
  - Applying the same prefix-scan fix to `add_songs_from_dir` and `load_songs_from_dir`.

- **Library artist/album view — name variants grouped separately**: Artists and albums with minor
  tag differences (differing case, extra whitespace, diacritics, or punctuation variants such as
  en-dashes or smart quotes) were stored under separate keys and appeared as duplicate entries in
  the library. Fixed by introducing a `normalize_name` function applied to all album and artist
  grouping keys at scan time and query time. Normalization covers: case folding, whitespace
  collapsing, diacritic stripping (NFD + drop combining marks), and common punctuation variants
  (en/em-dash → `-`, smart quotes → straight quotes). The original display title/artist is
  preserved as the first-seen value, so the library still shows "Pink Floyd" not "pink floyd".
  Backwards-compatible with existing databases: `find_by_id` tries the normalized key first and
  falls back to the verbatim key for records scanned before the fix.

- **Library artist/album view — "QuerySongsByAlbum" returns no results**: After the normalization
  fix, `find_by_id` looked up only the normalized sled key. Existing databases written before the
  fix still stored records under verbatim keys, so lookups returned nothing until a rescan.
  Fixed with a verbatim-key fallback in `find_by_id` so songs are returned immediately without
  requiring a rescan.

---

## v1.8.5 — 2026-03-03

### New Features

#### Media Key Support for Local Browser Playback
When using Local Browser Playback mode, the browser's [Media Session API](https://developer.mozilla.org/en-US/docs/Web/API/Media_Session_API) is now fully integrated.

- **Hardware media keys** (keyboard play/pause, next, previous) control rsplayer directly from the browser tab.
- **OS-level controls** — lock screen controls on Android/iOS, desktop media overlays, and Bluetooth headset buttons — all work out of the box.
- **Track metadata** (title, artist, album, artwork) is pushed to the OS media session so it appears in the system notification shade, lock screen, and connected media displays.
- **Seek support**: scrubbing via OS controls seeks the `<audio>` element directly.

### Improvements

- **Browser Playback — Progress Tracking**: The `<audio>` element now drives playback progress independently of backend `SongTimeEvent` messages. Backend time events are ignored in browser playback mode to prevent drift. Infinite/NaN duration values (radio streams) are handled gracefully.
- **Browser Playback — Playback State**: Pause, resume, and track-end events from the `<audio>` element are now reflected accurately in the UI player state, fixing cases where the UI showed stale state after the browser paused or buffered.
- **Browser Playback — Lyrics Sync**: Synchronized lyrics now track `<audio>` element time directly in browser mode, with ring buffer latency offset correctly zeroed out.
- **Ring Buffer Offset**: When Local Browser Playback is active, the ring buffer latency offset is set to zero (no hardware buffer to compensate for), preventing lyrics and progress from being offset.
- **Auto-advance**: When a track ends in the browser audio player, the next track in the queue is automatically played.

---

## v1.8.0 — 2026-03-02

### New Features

#### Synchronized Lyrics
Real-time synchronized lyrics support has been added via integration with **LRCLIB**.
- **Web UI Lyrics View**: A new lyrics modal (accessible via the alignment icon in the player) displays lyrics in real-time.
- **Auto-Scrolling**: The view automatically scrolls to keep the current line centered and highlighted.
- **Latency Compensation**: Synchronization accounts for the configured audio ring buffer size to ensure lyrics stay perfectly in sync with the audible sound.
- **Fallback**: Plain text lyrics are displayed if synchronized data is unavailable.

#### Local Browser Playback
A new "Local Browser Playback" mode allows you to stream audio directly to your web browser.
- **Listen Anywhere**: Play your music library on the device you're using to control rsplayer (phone, tablet, laptop) rather than the server's hardware output.
- **Full Control**: Volume, seeking, and playback state are synchronized between the browser and the backend.
- **Easy Toggle**: Switch between ALSA, PipeWire, and Browser playback directly from the settings page.

#### PipeWire Integration
Native support for PipeWire has been added for modern Linux distributions.
- **Virtual Card**: PipeWire appears as an available audio interface if `wpctl` is present on the host.
- **Volume Management**: Volume control is handled via `wpctl` for seamless integration with the system's sound server.

### Improvements

- **Metadata & Discovery**: Integration with **last.fm API** for enhanced metadata and improved fuzzy search capabilities in the library.
- **ALSA "Default" Device**: Improved compatibility by allowing the selection of the system's "default" ALSA device.
- **Feature Comparison**: Updated `FEATURE_PARITY.md` to include **Logitech Media Server (LMS)** and reflect new rsplayer capabilities.
- **Backend**: The backend now serves music files directly to the Web UI to support the new local playback feature.

### Bug Fixes

- Fixed various linting issues and removed unnecessary memory allocations (clippy fixes).
- Improved error handling for audio device selection.

---

## v1.7.0 — 2026-03-01

### New Features

#### Priority Queue
A persistent priority queue has been introduced that ensures selected songs or groups of songs play immediately after the current track, without disrupting the rest of the playback order.

- Add a **single song**, a **full album**, an **artist's discography**, a **directory**, or a **static playlist** to play next — directly from any library view
- Add and immediately play any of the above with the new "Add and Play" action
- Priority queue entries survive restarts and are cleared automatically when the queue is replaced or cleared

#### Drag-and-Drop Queue Reordering
Queue items can now be reordered by dragging them to a new position directly in the queue view. A "Play Next" action is also available per queue item to move it immediately after the current song.

#### Home Assistant Integration via WebSocket Custom Component
The backend now exposes a proper WebSocket endpoint suitable for use with a Home Assistant custom component, enabling real-time state streaming to HA without polling.

#### Firmware Power State Events via USB
The USB command channel now handles `PowerState=` messages from the firmware, emitting power state change events that can be acted upon by the rest of the system.

### Improvements

- **Queue view**: Song action menus now include "Add to queue", "Play Next", and "Add and Play" options consistently across the Artists, Files, and Static Playlists library pages
- **Queue clear/replace**: Both operations now also flush the underlying sled database to reclaim disk space from cleared log entries
- **Settings UI**: Power On / Power Off firmware buttons are now styled as small buttons to reduce visual weight
- **Feature parity docs**: Updated comparison table now includes **MPD** and **Navidrome** alongside Volumio and Roon; rsplayer's DSP engine and HA integration are reflected

### Bug Fixes

- Fixed a bug in priority queue processing where stale (already-removed) entries could cause the wrong song to be selected as next
- Fixed priority queue not being cleared when calling `replace_all` or `clear` on the queue service

### Removals

- **MQTT command channel settings** have been removed from the settings model. The USB command channel and the new WebSocket endpoint are the recommended integration paths going forward.

---

## v1.6.0 — 2026-02-23

See git tag `1.6.0` for changes prior to this release.
