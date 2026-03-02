# Release Notes

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
