# Backend Architecture (Developer Notes)

How the RSPlayer backend is put together: crate layout, the command/event
model, the audio pipeline, and storage. Multiroom sync has its own deep-dive
in [Multiroom Architecture](multiroom_architecture.md); this page covers
everything else. Module-level rustdoc (`//!` headers) in each source file
carries the per-file detail — this page is the map.

## Crate Map

| Crate | Responsibility |
|-------|----------------|
| `crates/api_models` | Shared serde data model: commands, events, settings, songs/albums. Crosses the WebSocket as JSON and is persisted — schema changes must stay backward-compatible |
| `crates/server` | The `rsplayer` binary: composition root, axum HTTP/WS server, command dispatch, network mounts |
| `crates/config` | `Settings` persistence (one JSON blob in fjall) with in-memory cache and schema migrations |
| `crates/playback` | The audio engine: Symphonia decode loop, cpal output (`AudioOutput`), DSD path, VU, multiroom tee/sink |
| `crates/metadata` | Library scanner, fjall repositories (songs/albums/stats/loudness), queue, playlists, radio metadata, APE/DSF/SACD Symphonia plugins |
| `crates/dsp` | Parametric EQ (biquads, CamillaDSP-derived) with a lock-free config handoff to the audio thread |
| `crates/sync` | Multiroom leader/follower over iroh QUIC — see the dedicated doc |
| `crates/hardware` | Volume-control devices (ALSA/PipeWire/software/firmware), USB front-panel link, LIRC remote |
| `crates/wire` | `no_std` protocol shared with the front-panel firmware repo (postcard + COBS over USB serial) |
| `crates/desktop` | Tauri wrapper: embeds the backend in-process, webview UI, OS media keys |
| `web-ui` | Dioxus web frontend (not covered here) |

Dependency direction (roughly): `server → {sync, playback, metadata, hardware, config, dsp} → api_models`. `playback` depends on `metadata` (probe/codec registries, loudness) and `dsp`; `sync` depends on `playback` (tee, sink); `hardware` depends on `wire`.

## Process Shape

One process, one tokio runtime, plus dedicated OS threads where blocking or
latency demands it:

- **tokio tasks**: axum HTTP/HTTPS servers, WebSocket fan-out, the two
  command handlers, the multiroom sync service, USB/LIRC glue.
- **Playback thread** (per play, `player_threads_priority`, never `Min` —
  `Min` starved audio on single-core devices): runs the decode loop in
  `symphonia.rs`.
- **cpal audio callback** (driver thread): drains the ring buffer, applies
  software volume.
- **Scanner thread** (per rescan) and the **loudness pool** (half the cores,
  pauses itself during playback).
- **Multiroom sink thread** (grouped follower) — same priority rule as
  playback.

## Command / Event Model

The system is a unidirectional loop: **commands in → state changes out**.

```
web UI / desktop media keys / USB panel / IR remote
        │  UserCommand (JSON over WS, or direct mpsc)
        ▼
  mpsc channel ──► command_handler (one at a time)
        │                 │ calls services (player, queue, metadata…)
        ▼                 ▼
  SystemCommand      StateChangeEvent ──► broadcast channel
  (volume/power)                            │
        ▼                                   ▼
  AudioInterfaceService          every WS client + USB panel + sync tee
```

- `UserCommand` (in `api_models::common`) is the single command entry enum;
  `command_handler.rs` routes to per-domain modules (`player_commands.rs`,
  `queue_commands.rs`, …) sharing one `CommandContext`.
- Queries are answered via the same broadcast `StateChangeEvent` channel —
  there are no request/response pairs, so all clients converge on identical
  state.
- Volume/power run on a separate `SystemCommand` channel against the
  hardware crate.
- While the instance is a grouped multiroom follower, an `AtomicBool`
  (`multiroom_follower_active`) makes `player_commands` reject local
  transport commands.

## Audio Pipeline

```
source ──► Symphonia decode ──► [multiroom tee] ──► AudioOutput.write()
                                                       │  per-format writer:
                                                       │  resample (rubato FFT)
                                                       │  → EQ (dsp crate)
                                                       │  → VU meter
                                                       ▼
                                             SPSC ring buffer (ring_buffer_size_ms)
                                                       ▼   blocking push = backpressure
                                             cpal callback: drain + software volume
                                                       ▼
                                          ALSA / PipeWire / CoreAudio / WASAPI / ASIO
```

Key points:

- **`AudioOutput`** (`playback/src/rsp/audio_output.rs`) is the single output
  path for every platform and both local + multiroom-sink playback. The name
  `alsa_output.rs` was retired in 2026-07 — it is pure cpal.
- **Source resolution** (`audio_source.rs`): local paths against the music
  dirs; HTTP with ICY metadata for radio; APE and SACD-ISO virtual tracks
  (`…#SACD_<n>`) via the custom readers in the metadata crate.
- **Sample-format negotiation**: the device is opened at the source rate if
  it supports it, otherwise the resampler targets an integer multiple
  (cleanest ratio) or the closest supported rate; retry ladders handle
  drivers that reject advertised configs.
- **DSD** never touches the PCM chain (no EQ, no VU, no software volume, no
  multiroom): `dsd.rs` types pass the raw bit stream to native-DSD cpal
  formats.
- **EQ handoff**: the command thread builds a fresh `Equalizer` and parks it
  in `DspHandle::pending`; the audio path swaps it in between writes —
  no lock contention on the hot path (`dsp/src/dsp_processor.rs`).
- **Loudness normalization**: measured in the background (EBU R128,
  `ebur128`), stored per song, applied as a gain filter inside the EQ chain;
  the gain also rides multiroom `StreamStart` so followers match.
- **Volume**: `VolumeCrtlType` selects ALSA mixer / PipeWire / software /
  firmware. Software volume is a cubic curve applied in the cpal callback
  (post-ring), so changes take effect within one device buffer.

## Storage

Everything persists in **one fjall LSM database** (`rsplayer.db`, cwd-relative
— the desktop app cds into the OS config dir first), one keyspace per
concern:

| Keyspace | Contents |
|----------|----------|
| `configuration` | `Settings` as one JSON value (see below) |
| `songs` | `Song` JSON keyed by library-relative path |
| `albums` | Albums keyed by normalized `artist\|album` |
| `play_statistics` | Play/skip/like counters per song key |
| `loudness` | Integrated LUFS per song key |
| `queue`, `queue_status`, `queue_random_history` | Queue items (insertion-ordered ids), current position/mode, random-mode history |
| `playlist`, `playlist_list` | Saved playlist items (`{name}_{index}`) and headers |
| `player_state` | Pause flag + last position for resume-on-restart |
| `multiroom` | The iroh endpoint secret key |

The settings blob round-trips **whole** through `GET/POST /api/settings`;
there is no merge. A stale client posting an old schema drops newer sections
— that is why every newer field has `#[serde(default)]` and serialized names
are frozen (the `alsa_*` settings names predate cross-platform support and
now configure cpal everywhere).

## HTTP / WebSocket Server

- axum; web UI embedded via rust-embed (debug builds read `dist/` from disk).
- `/api/ws`: commands in, every `StateChangeEvent` broadcast out to all
  clients.
- `/api/settings` (whole-struct GET/POST + validation), `/api/artwork/<id>`,
  range-capable audio streaming for local-browser playback, and an optional
  HTTPS listener.
- Audio-card enumeration is cached at startup: probing drivers (ASIO
  especially) can disrupt a live stream, so rescan only happens on explicit
  `?rescan=true`.
- **Degraded mode**: if the audio device cannot be opened at startup, only
  the settings/UI server starts so the device can be fixed remotely.

## Hardware Extras

- **USB front panel**: `hardware::usb` speaks the `wire` protocol (postcard
  + COBS, `no_std`-shared with the firmware repo) over USB serial — display
  mirroring out, knob/button commands in, auto-reconnect.
- **IR remote**: `ir_service` (feature `lirc`) reads the lircd socket.
- **Desktop app**: Tauri window around the same backend, free-port
  selection, souvlaki media-key integration (MPRIS2/MediaRemote/SMTC),
  graceful shutdown on window close.

## Testing

- `cargo make test_ci` (creates the `dist/web-ui/public/index.html` stub
  that rust-embed needs, then `cargo test`).
- Metadata services are tested against the `ports` traits with in-memory
  fakes (`metadata/src/ports/fakes.rs`); repositories get standalone fjall
  instances in temp dirs.
- Playback/sync unit tests cover the pure parts (resampling stretch, clock
  estimator, protocol round-trips); end-to-end audio verification is manual
  — see the release-testing notes in the multiroom doc.
