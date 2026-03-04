# Release Notes

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
