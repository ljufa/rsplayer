# Release Notes

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
