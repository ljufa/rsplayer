# Release Notes

## v4.2.0 — 2026-06-30

### New Features

#### Windows Support (experimental)

RSPlayer now builds and runs on Windows. Two artifacts are produced:

- **Headless server** (`rsplayer_windows_amd64.exe`) — a standalone executable you can run directly from a terminal or schedule as a Windows service. No installation needed.
- **Desktop installer** (`rsplayer-desktop_windows_amd64.exe`) — an NSIS installer that bundles the Tauri desktop app and automatically installs [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) if it is not already present (included with Windows 10/11 and Edge).

Audio output uses WASAPI via `cpal`. The web UI is served at `http://localhost:8000` and works in any browser (see troubleshooting.md if Edge shows a blank page — this is an Enhanced Security Mode issue, not an RSPlayer bug).

The CRT is statically linked (`-C target-feature=+crt-static`) so the binary has no dependency on `VCRUNTIME140.dll`.

Platform limitations on Windows: network share mounting, ALSA/PipeWire volume, IR remote, system poweroff/reboot, and firmware USB integration are unavailable.

#### Media Key Bindings in Desktop App

The desktop application now registers with the OS media session, so hardware media keys and OS-level controls (lock screen, Bluetooth headset, keyboard media row) work out of the box:

| Key / Event | Action |
|---|---|
| Play / Pause / Toggle | Toggle play/pause |
| Next | Next track |
| Previous | Previous track |
| Stop | Stop |
| Set volume | Set volume (0–100) |

On **Linux** this uses MPRIS2 (D-Bus), on **macOS** the native MediaRemote framework, and on **Windows** the GlobalSystemMediaTransportControls API — all via the `souvlaki` crate. The media session is registered for the lifetime of the app; no configuration is needed.

#### Remote Access URL in Desktop Settings

When running as a desktop app, the Settings page now shows a **Remote access** link below the version string. It displays the machine's local network address (e.g. `http://192.168.1.42:8000`) so you can open the web UI from a phone or another device on the same LAN without having to look up the IP manually.

### Improvements

#### UX — First-Time Setup and New-User Guidance ([#20](https://github.com/ljufa/rsplayer/issues/20))

Several changes improve the out-of-the-box experience for new users, especially on the desktop app.

**Welcome modal expanded with setup steps**

The first-time welcome modal now walks through a structured setup checklist:

1. **Audio Device** (required) — direct link to Settings to select a playback device.
2. **Music Library** (required) — configure local or network music directories.
3. **Network Mounts** (optional, shown only on Linux/ALSA builds) — brief explanation that network share configuration is only available in the headless server install.
4. **Internet Radio** (optional, shown only where supported) — explains that internet radio requires a network-connected server build and is unavailable in the desktop app.
5. **Start Listening** — confirms setup is complete.

This directly addresses the confusion reported in [#20](https://github.com/ljufa/rsplayer/issues/20): users installing the desktop app before the server expected radio stations and network mounts to work, but those features are only available in the headless server.

**UI preferences moved from browser localStorage to backend database**

Theme, visualizer style, background image toggle, and the "welcome shown" flag are now stored in the backend database via the existing settings API, rather than `localStorage`. This fixes the welcome modal never appearing in the Tauri desktop app, where the webview does not reliably persist `localStorage` across restarts. Preferences are now fully synchronized with the server and survive app restarts on all platforms.

**Radio — Favorites empty state now guides users to browse**

When the Favorites tab is empty, the Radio page now shows a descriptive empty state pointing to the Top/Country/Language/Search tabs instead of a blank list. Previously the empty state never rendered at all because the loading flag was not cleared for empty favorites.

#### Remote Access URL — Copy Button and Improved Description

The remote access URL shown at the bottom of the Settings page (desktop mode) has been improved:

- The URL is now displayed as **plain text** instead of a clickable link — clicking the link previously redirected the Tauri webview away from the app.
- A **copy button** (clipboard icon) is shown next to the URL for easy copying to another device.
- The description now reads *"Open on another device (phone, tablet, browser):"* to make the purpose immediately clear.

#### Desktop App — Default Port Changed to 8001

The desktop application now starts on port **8001** by default (previously 8000). This avoids an accidental port conflict if the standalone headless server binary is also run on the same machine without a custom `PORT` environment variable — both previously defaulted to 8000. The server install (deb/rpm/systemd) continues to use port 80 via the `env` file and is unaffected.

### Internal / Build

- **cpal upgraded to 0.18.1** — the audio output library has been updated to 0.18.1. The fork's custom patches (DSD native output, ALSA buffer-size constraint fix, high-sample-rate support) have been rebased onto the new branch (`v_0.18.1_patched`).
- **Cross-platform signal handling** — `tokio::signal::unix` (`SIGTERM`) is now wrapped in `#[cfg(unix)]` so the server crate compiles cleanly on Windows. On non-Unix platforms the terminate-signal future resolves never (`std::future::pending`), and Ctrl+C continues to work via `tokio::signal::ctrl_c` on all platforms.
- **Windows CI job** (`build_desktop_windows`) added to `cd.yml`. Runs on `windows-latest`, installs `tauri-cli`, builds the headless `.exe` and the NSIS installer, and uploads both as release artifacts. Dispatch target `windows` is available alongside `macos`, `all`, and the Linux per-arch targets.
- **Comparison table** in `README.md` extended with Language, Playback engine, OS support, and Native desktop app variant rows.

---

## v4.1.0 — 2026-06-23

### New Features

#### Configurable Bind Address

The server's listen address is now controlled by the `BIND_ADDR` environment variable (default `0.0.0.0`). Previously the address was hardcoded to bind on all interfaces. Setting `BIND_ADDR=127.0.0.1` restricts the server to loopback only — useful when running behind a reverse proxy or in an environment where external exposure is undesirable.

The variable is present in `/opt/rsplayer/env` with its default value and is applied consistently to the HTTP listener, the HTTPS listener, and the degraded-mode fallback server.

### Improvements

#### Desktop Mode — Settings Page Cleaned Up

Two settings sections are now hidden when the desktop app is running:

- **RSPlayer firmware control channel** (USB section) — irrelevant on a desktop host that has no attached RSPlayer firmware.
- **System** (power controls — Restart RSPlayer, Restart system, Shutdown) — these system-level commands target the headless server use-case; they have no meaning in the desktop context.

The version string that was previously embedded inside the System section is now shown unconditionally at the bottom of the settings page, so it remains visible in all modes.

The desktop app now sets the `RSPLAYER_DESKTOP` environment variable at startup, which the backend reads and exposes as the `desktop_mode` flag in the `Settings` response. The web UI uses this flag to conditionally render the sections above.

### Internal / Build

- `composition_root::build()` renamed to `build_app_container()` for clarity.
- New `run_desktop_dev` Makefile task: builds the release UI, copies `loading.html`, and runs `cargo tauri dev` — a single command for local desktop development iteration.
- `install.sh`: downloaded package files are now `chmod 644` after a successful download, ensuring consistent permissions across package managers.
- Removed a stale `#[allow(clippy::redundant_pub_crate, clippy::too_many_lines)]` attribute from `main.rs`.

---

## v4.0.1 — 2026-06-19

### New Features

#### Desktop Arch Linux Package

RSPlayer Desktop is now available as a `.tgz` package for Arch Linux (and derivatives like Manjaro):

- `rsplayer-desktop_<ver>_amd64.tgz` is produced alongside the existing deb and rpm desktop packages.
- The tarball includes the `rsplayer-desktop` binary, a `.desktop` entry, and icons for 32px, 128px, and 256px.
- A `PKGBUILD` and install script are placed alongside the tarball for users who prefer to build from source or inspect the dependency list.
- `install_desktop.sh` detects Arch-based distros, installs runtime dependencies (`webkit2gtk-4.1`, `gtk3`, `libappindicator-gtk3`, `librsvg`, `alsa-lib`), extracts the tarball, and updates the icon cache.

Platform coverage:

| Platform | Architectures | Package Format |
|----------|--------------|----------------|
| Linux | x86_64 | `.deb`, `.rpm`, `.tgz` |
| macOS | Apple Silicon, Intel | `.dmg` |

#### New Desktop Installer Script

A new `install_desktop.sh` script is the recommended way to install the desktop application on any supported platform. It auto-detects the distribution, downloads the correct desktop package from the latest GitHub release, and installs it with the appropriate package manager — separate from `install.sh` (which remains the headless server installer).

### Bug Fixes

#### Desktop Packages Missing Icons (deb/rpm)

The `tauri.conf.json` was missing the `icon` field in the `bundle` section. Without it, Tauri's bundler did not include any icon files in the generated `.deb` and `.rpm` packages, so the application appeared without an icon in the system menu. Fixed by adding `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, and `icons/icon.png` to the bundle configuration.

#### Desktop App — "asset not found: loading.html"

The desktop application referenced `loading.html` (the startup splash screen) from Tauri's `frontendDist`, but the file was never copied into the frontend distribution directory. The `bundle_desktop_release` Makefile task now copies `crates/desktop/loading.html` to `dist/web-ui/` before invoking `cargo tauri build`.

#### Server Installer — Ubuntu Downloading Desktop Package

`install.sh` was downloading the desktop `.deb` (`rsplayer-desktop_*.deb`) instead of the server `.deb` (`rsplayer_*.deb`) on Ubuntu/Debian systems because the grep pattern `_<suffix>.deb` matched both asset names. The asset filter now explicitly excludes paths containing `/rsplayer-desktop_`.

### Internal / Build

- The `bundle_desktop_release` Makefile task now uses absolute paths (`$(pwd)/../../...`) and runs the tarball creation in a subshell, preventing working-directory changes from breaking subsequent commands.
- CI job name updated to reflect the additional arch tgz output.

---

## v4.0.0 — 2026-06-18

### New Features

#### Desktop Application (Tauri)

RSPlayer now ships as a standalone desktop application alongside the headless server:

- **Native window** built with [Tauri 2](https://v2.tauri.app/) — deb and rpm packages for Linux (`rsplayer-desktop_*_amd64.deb`, `rsplayer-desktop-*.x86_64.rpm`), and a DMG for macOS.
- The desktop app embeds the same RSPlayer backend (from `crates/server/src/lib.rs`) and runs it as a child tokio task inside the Tauri application process.
- On startup the window shows a loading screen (`loading.html`) while the backend starts up; once the HTTP server is ready it redirects to the web UI.
- Graceful shutdown: closing the desktop window sends a shutdown signal to the backend, triggering a clean database `SyncAll` persistence.
- If the configured port (default `8000`) is already in use, the app probes sequential ports up to `9000` before falling back to an OS-assigned port.
- A new `install_desktop.sh` script detects the platform (Debian/RPM/macOS) and downloads the appropriate desktop package from the latest GitHub release.

The web UI now shows a **"Connecting to server…"** full-screen overlay with a spinner while the WebSocket is not yet connected — essential for the desktop build where the backend starts asynchronously in the same process.

**Desktop is available for:**

| Platform | Architectures | Package Format |
|----------|--------------|----------------|
| Linux | x86_64 only | `.deb`, `.rpm` |
| macOS | Apple Silicon, Intel | `.dmg` |

> The headless server packages (`rsplayer_*` — deb, rpm, Arch tgz, raw binary) continue to be produced for all architectures and are unaffected.

### Internal / Build & Release Pipeline

#### Packaging overhaul — standard tooling instead of hand-rolled scripts

The release pipeline was simplified and made more robust:

- **Version single source of truth** moved from `Makefile.toml` to `[workspace.package]` in the root `Cargo.toml`. `cargo-deb`, `cargo-generate-rpm`, `cargo-packager` and the application itself (`env!("CARGO_PKG_VERSION")`) all read it natively. CI fails fast if a release tag doesn't match the workspace version.
- **RPM packages** are now built with `cargo-generate-rpm` (declarative config in `crates/server/Cargo.toml`) instead of a hand-rolled `rpmbuild` spec-template pipeline. The old `PKGS/rpm/rsplayer.spec.in` was removed. Install/uninstall scriptlets and file lists are unchanged.
- **macOS** server builds (`rsplayer_darwin_arm64`/`rsplayer_darwin_amd64`) and desktop DMGs are now built **natively on GitHub macOS runners**. The osxcross-based darwin cross-compilation Docker images and entry-point scripts were removed.
- **All server packages** and raw binaries keep their previous release asset name patterns; `install.sh` and the Docker publish flow are unaffected.

#### Docker Builder Image Consolidation

- The separate `rsplayer-backend-builder` and `rsplayer-ui-builder` Docker images are merged into a single `rsplayer-builder` image (`docker/Dockerfile.builder`), which includes everything needed for both the web UI and backend/desktop builds (Rust with wasm32 target, Node.js, Tailwind CLI, cross, cargo-deb, cargo-generate-rpm, cargo-packager, tauri-cli, dioxus-cli, cargo-make, and all Linux desktop/WebKit dev libraries).
- The legacy `docker/Dockerfile.backend` was removed.
- All darwin cross-compilation images (`cross-aarch64-apple-darwin`, `cross-x86_64-apple-darwin`) were removed from `build-images.yml`.

#### CI/CD Consolidation

- The `cd.yml` workflow was rewritten: it now supports `workflow_dispatch` with per-target selection, verifies the pushed tag matches the workspace version, and produces all release assets (deb, rpm, Arch tgz, binary) via the new `cargo make package_linux_release` task.
- A new `build_desktop_linux` job produces deb+rpm desktop packages via Tauri.
- The legacy `copy_remote_mac` Make task was removed (macOS builds are now done natively).

### Improvements

#### Server Refactored for Embedding

The server binary logic was extracted from `main.rs` into a new `lib.rs` (`crates/server/src/lib.rs`) exposing a public `run_backend()` function. This allows the desktop application (and any future embedders) to start the server as a library rather than spawning a subprocess. The `build.rs` that previously parsed the version from `Makefile.toml` and embedded `index.html` via a manual `include_str!` was removed — the version now comes from `env!("CARGO_PKG_VERSION")` and `index.html` is served directly from the `RustEmbed`-based `StaticContentDir`.

#### Rust Edition 2024

The `crates/desktop` crate uses the Rust 2024 edition. The server and other workspace crates were updated to edition 2024 as well.

#### Rustfmt Line Width Extended

The project-wide `max_width` was increased from 120 to 140 characters.

#### Documentation Site Overhaul

- New **logo** (`docs/_assets/logo.svg`) with an engraved "RSP" hexagon design.
- Docsify theme switched from `buble` to `vue`, with updated styling for logo placement and inline images.
- **Full-text search** plugin (`docsify-search`) added to the documentation site.
- **Image zoom** plugin (`docsify-zoom-image`) added.
- `installation.md` rewritten with a supported-platforms table (Server vs Desktop per architecture), explicit release filename suffixes, and separate macOS server/desktop instructions.

### Removals

- **Darwin cross-compilation infrastructure** — `docker/Dockerfile.cross-aarch64-apple-darwin`, `Dockerfile.cross-x86_64-apple-darwin`, `docker/darwin-entry.sh`, and `docker/darwin-x86_64-entry.sh` removed (macOS is now built natively).
- **`PKGS/rpm/rsplayer.spec.in`** — replaced by `cargo-generate-rpm` declarative config in `crates/server/Cargo.toml`.

---

## v3.5.5 — 2026-05-30

### Bug Fixes

#### Library/Artists — Orphaned Album Entries Left After File Deletion

Removing music files from disk and running "Update Library" (incremental scan) left empty album entries in the artist view. The tracks were correctly removed from the database, but the album record was never cleaned up — albums with zero songs remained visible in the Library/Artists tree with no playable content.

**Root cause:** When the scanner detected a file was deleted, it only removed the entry from the `songs` keyspace. The corresponding `albums` keyspace was never updated: the album's `song_keys` list retained the deleted track's file path, and albums that became empty were never deleted.

**Fix (two parts):**

1. **Per-track album cleanup during scan** — Before deleting a song from the database, the scanner now reads its metadata, removes the file path from the owning album's `song_keys`, and deletes the album entry if `song_keys` becomes empty. This handles all future incremental scans.

2. **One-shot migration for existing databases** — On first startup after upgrading to 3.5.5, `MetadataService::new()` runs a migration that iterates every album in the database, strips any `song_keys` entries referencing songs that no longer exist, and removes albums that become empty. A persistent marker is stored in the `_migrations` keyspace so this runs exactly once.

> ⚠️ **Action required:** The migration runs automatically on first startup. No manual rescan is needed — existing orphaned albums will be cleaned up immediately. Users who prefer to trigger a full rescan (Settings → Rescan — Full) will also see the same result.

---

## v3.5.4 — 2026-05-29

### Internal / Architecture

#### Project Structure Restructured — Flat to `crates/` Layout

The source tree has been reorganized from a flat list of crate directories to a standard `crates/` workspace layout:

| Before | After |
|--------|-------|
| `rsplayer_backend/` | `crates/server/` |
| `rsplayer_playback/` | `crates/playback/` |
| `rsplayer_metadata/` | `crates/metadata/` |
| `rsplayer_hardware/` | `crates/hardware/` |
| `rsplayer_config/` | `crates/config/` |
| `rsplayer_dsp/` | `crates/dsp/` |
| `rsplayer_wire/` | `crates/wire/` |
| `rsplayer_api_models/` | `crates/api_models/` |
| `rsplayer_web_ui/` | `web-ui/` (workspace member) |

Every crate's internal module structure and public API are unchanged — this is purely a file-system reorganization. All `Cargo.toml` workspace paths, `Cross.toml` volume mounts, and CI workflow paths have been updated to match.

#### `build.rs` for Version Propagation

Version is now centralized in `Makefile.toml` (`RELEASE_VERSION`) and propagated into the server binary at compile time via a `build.rs` in `crates/server/`. The old `rsplayer_backend/build.rs` has been removed. The build script also embeds `index.html` from the UI release build directly into the server binary.

#### Makefile and CI Consolidation

- `Makefile.toml` streamlined — build tasks reorganized, redundant targets removed.
- `web-ui/Makefile.toml` removed; web UI tasks merged into the root `Makefile.toml`.
- CI/CD workflow updated for the new directory layout (`crates/server/`, `web-ui/`).
- Docker images and `Cross.toml` updated with corrected paths.

#### `filters.rs` Refactored

The biquad filter implementation in `crates/dsp/src/filters.rs` received a structural cleanup — coefficient computation was factored into reusable methods, and shared math was deduplicated across filter types.

### Improvements

#### Frontend Cross-Arch Build Support

Player page now supports a `cross_arch_build` feature toggle that allows disabling platform-specific widget rendering, making the frontend buildable for broader WASM targets.

---

## v3.5.0 — 2026-05-22

### New Features

#### macOS Build Target Support (Apple Silicon + Intel)

RSPlayer now has cross-compilation targets for macOS:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

Build and release pipeline support was added across `Cross.toml`, custom cross Docker images, and GitHub Actions. Tag releases now produce darwin binary artifacts alongside Linux outputs.

### Improvements

#### Software Volume Control Path

A new software volume backend (`VolumeCrtlType::Software`) was added and integrated end-to-end:

- New software gain volume device with `0..100` range and 5-step increments.
- Software volume level is shared via atomic state and initialized from saved settings at startup.
- PCM attenuation is applied in the output callback using a perceptual cubic curve `(vol/100)^3`, so volume changes take effect with output-buffer latency instead of ring-buffer latency.
- Volume-control options are now platform-aware in settings. Linux keeps ALSA/Pipewire/software/off options (based on build features); non-ALSA builds expose software/off.

#### Non-Linux Audio Device and Settings UX

- Backend now enumerates output devices via `cpal` when ALSA is not enabled and exposes a stable `System Default` output entry.
- Settings UI now consumes backend-provided available volume control types instead of hardcoded enum iteration.
- ALSA-only controls (for example, ALSA buffer size) are hidden when ALSA is unavailable.
- Changing audio card now auto-selects the first PCM device to avoid empty output-device names.

#### CI/CD and Tooling for Darwin Targets

- Added darwin targets to release workflow matrix.
- Linux packaging remains unchanged (`.deb`, `.rpm`, `.tgz`), while darwin targets publish binary artifacts only.
- Added two new builder-image jobs in `build-images.yml`:
  - `rsplayer-cross-aarch64-apple-darwin`
  - `rsplayer-cross-x86_64-apple-darwin`
- `build_release` now applies `--no-default-features` only for darwin targets.

### Bug Fixes

#### Network Mount Responsiveness on Unavailable Shares

Network-mount behavior was hardened to avoid long blocking stalls when remote storage is unavailable:

- NFS mounts now use bounded retry/timeout options (`soft,timeo=50,retrans=2`).
- SMB mounts now preflight TCP connectivity to port 445 with a short timeout and mount with `soft` behavior.

These changes significantly reduce the time spent waiting on unreachable network shares and improve overall player responsiveness.

#### Platform Compatibility and Runtime Safety

- Mount service is now Linux-specific, with a non-Linux stub implementation to keep non-Linux builds functional.
- Network mount management UI is hidden when mounts are not supported by the running platform.
- Poweroff/reboot commands now return a user-facing error notification on non-Linux platforms instead of attempting unsupported system commands.
- Rustls provider initialization was made explicit (`ring` default provider install) to match `axum-server` TLS configuration.
- Database persistence on shutdown was tightened from `SyncData` to `SyncAll`.

#### Metadata Scanning on 32-bit Architectures

Fixed a crash when scanning large audio files (> 2 GB) on 32-bit ARM targets (`arm-unknown-linux-gnueabihf`, `armv7-unknown-linux-gnueabihf`). The file-size check used `u32` arithmetic that wrapped around, causing the scanner to misclassify valid files as corrupt and abort the scan. The affected field in `metadata_service.rs` is now widened to `u64`.

### Internal / Architecture

#### Symphonia Upgraded to 0.6.0

The audio decoding engine has been updated from the project's custom Symphonia fork (based on 0.5.x) to the official [Symphonia 0.6.0](https://github.com/pdeljanov/Symphonia) release, rebased on top of it.

- The custom patches retained from the fork are:
  - **FLAC channel-count validation** — guards the FLAC decoder against a subframe decode panic when a stream advertises more channels than are present (upstream regression not yet ported to the 0.6.x line).
  - **MP3 demuxer infinite-loop guard** — breaks out of the strict-frame-sync loop after 10 consecutive failures on corrupted streams.
- API changes absorbed internally: `AudioDecoder::decode_ref` / `PacketRef`, new required `FormatReader::media_info`, `SeekTo::Timestamp` rename. These affect only the project's custom APE, DSD, and SACD demuxers/decoders and have no visible effect on playback behaviour.

#### cpal Upgraded to 0.17.3

The audio output library has been updated to [cpal 0.17.3](https://github.com/RustAudio/cpal). The fork's custom patches (DSD native output, ALSA buffer-size constraint fix, high-sample-rate support) have been rebased onto the upstream release tag.

#### CamillaDSP Replaced with Self-Contained Filter Implementation

The [CamillaDSP](https://github.com/HEnquist/camilladsp) library dependency has been removed. RSPlayer only used a small subset of CamillaDSP — biquad filters and a gain stage — and the library unconditionally links `alsa-sys 0.3.x` on Linux, which conflicts with the `alsa-sys 0.4.x` required by cpal 0.17.x.

The DSP subsystem (`rsplayer_dsp`) now contains a self-contained implementation of all required filter types:

- Biquad filters: Highpass, Lowpass, Bandpass, Notch, Allpass, Peaking EQ, High/Low shelf (Q and slope variants), first-order High/Lowpass and shelf variants, Linkwitz Transform.
- Gain filter with linear/dB/mute/invert modes.
- Direct Form II Transposed biquad with subnormal flushing.

Formulas are taken directly from the Audio EQ Cookbook (R. Bristow-Johnson). The public API of `rsplayer_dsp` is unchanged.

This change removes approximately 560 lines from `Cargo.lock` and eliminates the `alsa-sys` version conflict entirely.

#### Cross-Compilation Fix — `libLLVM` in Docker Containers

Fixed a build failure when cross-compiling for ARM targets using the project's custom `ghcr.io/ljufa/rsplayer-cross-*` Docker images. The `libc` build script spawns `rustc` as a subprocess; inside the container `$RUSTC` resolves to the real compiler binary rather than the rustup wrapper, so `LD_LIBRARY_PATH` was never set and the Rust toolchain's `libLLVM.so` could not be found.

`Makefile.toml` now evaluates the active toolchain's sysroot at build time and passes the lib directory into the container via `CROSS_CONTAINER_OPTS`, mounting it at the same host path and setting `LD_LIBRARY_PATH`. No Docker image rebuilds are required.

---

## v3.0.0 — 2026-05-05

### Breaking Changes

#### Firmware Protocol — Binary Wire Format (postcard + COBS)

The USB serial protocol between RSPlayer and the hardware firmware has changed from a human-readable text format (e.g. `SetTrack(title|artist|album)\n`) to a typed binary format using [postcard](https://docs.rs/postcard) serialization with COBS framing.

The new protocol is defined in the shared `rsplayer_wire` crate (a `no_std` library used by both the host and the firmware). All messages are now strongly-typed (`HostToFw` / `FwToHost` enums), length-bounded with `heapless::String`, and framed at up to 256 bytes per packet.

> ⚠️ **Action required:** Firmware must be updated to the matching version that speaks the new binary protocol. Running v3.0.0 against old firmware will result in garbled communication.

---

### New Features

#### Browser Audio Playback

RSPlayer can now stream audio directly to the browser over HTTP, without requiring an ALSA output device on the server. This makes it practical to run RSPlayer on a headless server or NAS and listen through any browser tab.

- Select **Browser** in the audio output dropdown (Settings → Audio Output) to switch to browser playback mode.
- The browser creates a hidden `<audio>` element that fetches tracks via `/music/<path>` and plays them locally.
- The player state (current song, play/pause, progress) continues to be driven by the backend over WebSocket; the browser element follows it.
- RSPlayer engine settings (input buffer, ring buffer, thread priority, ALSA buffer size) are hidden when browser playback is selected, as they are irrelevant in that mode.

#### Typed System Commands Over WebSocket

Volume and power commands from the frontend are now sent as a typed `SystemRequest` enum (`VolUp`, `VolDown`, `SetVol`, `ToggleMute`, `PowerOff`, `RestartSystem`, `RestartRSPlayer`, `SetFirmwarePower`) rather than ad-hoc strings. This closes a class of silent failures where an unrecognised command string would be dropped without feedback.

---

### Internal / Architecture

#### Web Server — warp → axum

The HTTP and WebSocket server has been rewritten from [warp](https://docs.rs/warp) to [axum](https://docs.rs/axum). The external API surface (REST endpoints, WebSocket protocol, static file serving) is unchanged. The new server adds:

- `tower-http` middleware for response compression and CORS.
- Byte-range support for the `/music/<path>` endpoint, enabling browser `<audio>` seeking.
- Cleaner WebSocket connection lifecycle with per-connection IDs and an active-user counter.

#### Composition Root

All service wiring that previously lived inline in `main.rs` has been extracted to `composition_root.rs`. The `AppContainer` struct owns every service and channel; `BuildOutcome::Degraded` captures audio-interface startup failures gracefully instead of panicking.

#### Repository Ports (Trait Abstractions)

`rsplayer_metadata` now exposes its repositories as traits (`AlbumRepository`, `SongRepository`, `LoudnessRepository`, `PlayStatisticsRepository`) behind `Arc<dyn …>` type aliases. Concrete fjall-backed implementations live alongside in-memory fakes (`InMemorySongRepository`, etc.) that can be used in unit tests without a real database on disk.

#### `PlaybackMode` Moved to `rsplayer_wire`

`PlaybackMode` is now defined once in `rsplayer_wire` (the shared `no_std` crate) and re-exported from `api_models`. This eliminates the duplicate definition that previously existed between the host API models and the firmware protocol.

---

## v2.9.6 — 2026-04-27

### Bug Fixes

#### Library/Artists — Albums with Common Names Disappear (e.g. "Greatest Hits")

Artists who had only one album with a common title (such as "Greatest Hits", "Best Of", or any name shared by multiple artists) were invisible in the Library/Artists view. Artists with multiple albums were shown, but the commonly-named album was missing from their list.

**Root cause:** The album database key was derived solely from the normalised album title (`normalize_name(album)`). Every artist whose album shared a title wrote to the same key, with the last-scanned artist overwriting all previous entries. Artists whose only album was overwritten ended up with zero DB entries and therefore never appeared in the artist list.

**Fix:** The album key is now artist-qualified: `normalize_name(artist)|normalize_name(album)`. Each artist's albums occupy their own key namespace, so "Greatest Hits" by Happy Mondays and "Greatest Hits" by Yello are stored and retrieved independently.

> ⚠️ **Action required:** A full library rescan (Settings → Rescan — Full) is required after upgrading to rebuild the album database with the new keys. Without it, the artist and album views will remain empty.

Additional related fixes included in this change:

- `find_by_artist` now sets `album.id` from the DB key (previously only the value was read, so the compound key was never propagated to callers).
- Genre and decade song lookups in the queue handler now use `album.id` instead of `album.title` when resolving albums, preventing the same collision for those code paths.
- `MetadataLibraryItem::Album` gains an `id` field (backward-compatible, `serde(default)`) that carries the compound DB key to the UI, so all album operations (expand, queue, load) use the correct key rather than the display title.

#### WebSocket Broadcast Channel Overflow During Large Library Scans

When scanning a large library, the backend sent one `MetadataSongScanned` progress event per file. With the top-level broadcast channel capacity of 20, the channel saturated almost immediately. Any `MetadataLocalItems` response (e.g. the artist list) sent while the channel was full was silently dropped by Tokio's broadcast receiver, leaving the Library/Artists page stuck in a loading state even though the data was correctly in the database. Users with large libraries on slow storage (e.g. Samba network shares) were most affected.

- Scan progress events now fire every 100 files instead of every file, matching the existing flush cadence.
- Broadcast channel capacity increased from 20 to 64 as a secondary defence against burst overflow.

### Improvements

#### Album Release Year — Year Only

Album entries in the Library/Artists tree previously displayed the full internal timestamp (e.g. `Greatest Hits (1999-01-01 00:00:00 UTC)`). They now show only the year: `Greatest Hits (1999)`.

#### Taller Bottom Player Bar

The footer mini-player bar has increased vertical padding (`py-1` → `py-3`) for a more comfortable tap target and better visual presence.

#### Library Files — No Scroll Reset on Directory Expand

Expanding a directory node in Library/Files and Library/Artists no longer resets the scroll position to the top of the list. The spurious `loading = true` signal write that triggered a full re-render on every expand has been removed.

---

## v2.9.5 — 2026-04-26

### Bug Fixes

#### Metadata Scan Crash — Whitespace-Only Album Tag

Library scans crashed with `key may not be empty` (lsm-tree panic) when a file contained an album tag consisting entirely of whitespace (e.g. `" "` or a non-breaking space). The raw value passed the existing `!a.is_empty()` guard, but `normalize_name` — which strips diacritics, lowercases, and collapses whitespace via `split_whitespace` — reduced it to an empty string. Passing an empty byte slice to `Keyspace::insert` triggered an unconditional panic inside lsm-tree, poisoning the fjall journal lock and aborting the scan.

Three guards were added:

- `album_repository::update_from_song`: returns `Ok(())` early if `normalize_name(album)` is empty, skipping albums whose title normalises to nothing.
- `album_repository::find_by_id`: returns `None` early if the normalised key is empty, preventing an empty-key read.
- `song_repository::save`: returns `Err` if `song.file` is empty rather than letting lsm-tree panic, turning an unrecoverable crash into a logged, skippable error.

### Build / CI-CD

#### Containerised CI/CD Pipeline

CI and CD workflows now run entirely inside Docker containers pulled from GHCR, replacing the previous approach of manually provisioning tools on self-hosted runners. Any generic runner with Docker available can now execute the full pipeline.

- **`rsplayer-backend-builder`** — Rust toolchain, `cargo-make`, `cargo-deb`, `cross 0.2.5`, and `docker.io` pre-installed.
- **`rsplayer-ui-builder`** — Rust + wasm32 target, Dioxus CLI, Node.js LTS, and Binaryen pre-installed.
- **Custom cross images** (`rsplayer-cross-armv6/7`, `rsplayer-cross-aarch64`, `rsplayer-cross-x86_64`, `rsplayer-cross-riscv64`) — each extends the matching `cross-rs 0.2.5` base image with the target-arch ALSA and OpenSSL development libraries baked in. This replaces the previous `pre-build` hooks in `Cross.toml`, which were incompatible with Docker-in-Docker environments because `cross` generated Dockerfiles at container-side paths that the host Docker daemon could not access.
- `Cross.toml` no longer contains any `pre-build` entries; every target points directly to a pre-built GHCR image.
- A new **`build-images.yml`** workflow rebuilds and pushes all builder and cross images to GHCR whenever a `Dockerfile.*` changes.
- **Dependabot** is configured for monthly updates across GitHub Actions, Docker images, and Cargo dependencies.
- Backend cross-compilation jobs now run in **parallel** across available runners (the previous `max-parallel: 1` serialisation is removed).
- Node.js 20 deprecation warnings eliminated by setting `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: true` in all workflows.

---

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
