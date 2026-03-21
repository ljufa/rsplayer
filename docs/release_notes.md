# Release Notes

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
