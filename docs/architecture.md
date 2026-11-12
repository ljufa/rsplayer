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
| `crates/config` | `Settings` persistence (one JSON blob in fjall) with in-memory cache and schema migrations; first launch persists platform-aware defaults supplied by the server (from `hardware::platform`) |
| `crates/playback` | The audio engine: Symphonia decode loop, cpal output (`AudioOutput`), DSD path, VU, multiroom tee/sink |
| `crates/metadata` | Library scanner, fjall repositories (songs/albums/stats/loudness), queue, playlists, radio metadata and saved radio stations, APE/DSF/DFF/SACD/Opus Symphonia plugins |
| `crates/podcast` | Podcast subscriptions: directory search (iTunes / Podcast Index), RSS/Atom feed refresh (`feed-rs`), episode cache in fjall, per-episode resume position and played state |
| `crates/scrobble` | Opt-in scrobbling to ListenBrainz and Last.fm: a pure state machine over player events (now playing, listened time, scrobble threshold), blocking `ureq` clients (Last.fm calls signed with the user's API secret), a per-service retry queue in fjall |
| `crates/dsp` | Parametric EQ (biquads, CamillaDSP-derived) with a lock-free config handoff to the audio thread |
| `crates/sync` | Multiroom leader/follower over iroh QUIC — see the dedicated doc |
| `crates/hardware` | Volume-control devices (ALSA/PipeWire/software/firmware), USB front-panel link, LIRC remote, platform/sandbox detection with first-launch playback defaults |
| `crates/wire` | `no_std` protocol shared with the front-panel firmware repo (postcard + COBS over USB serial) |
| `crates/desktop` | Tauri wrapper: embeds the backend in-process, webview UI, OS media keys |
| `web-ui` | Dioxus web frontend (not covered here) |

Dependency direction (roughly): `server → {sync, playback, metadata, podcast, hardware, config, dsp} → api_models`. `playback` depends on `metadata` (probe/codec registries, loudness) and `dsp`; `sync` depends on `playback` (tee, sink); `hardware` depends on `wire`. `podcast` depends only on `api_models` and `config`; the server adapts it to playback's `ResumePositionProvider` so the two never depend on each other. `scrobble` also depends only on `api_models` and `config`: it learns everything from the `StateChangeEvent` broadcast.

## Process Shape

One process, one tokio runtime, plus dedicated OS threads where blocking or
latency demands it:

- **tokio tasks**: axum HTTP/HTTPS servers, WebSocket fan-out, the two
  command handlers, the multiroom sync service, USB/LIRC glue, and the
  scrobble service (an event task that only runs the state machine, and a
  worker that makes the HTTP calls on the blocking pool, so a slow or
  unreachable service never touches playback).
- **Playback thread** (per play, `player_threads_priority`, never `Min` —
  `Min` starved audio on single-core devices): runs the decode loop in
  `symphonia.rs`.
- **cpal audio callback** (driver thread): drains the ring buffer, applies
  software volume.
- **Scanner thread** (per rescan) and the **loudness pool** (half the cores,
  pauses itself during playback).
- **Multiroom sink thread** (grouped follower) — same priority rule as
  playback.

### Desktop and Android wrapper

`crates/desktop` is a Tauri 2 shell around the same backend, built as a
library (`rsplayer_desktop_lib::run`) with a thin `main.rs` for desktop and
the Tauri mobile entry point for Android:

- It moves the cwd to a per-user data dir (`dirs::config_dir()/rsplayer` on
  desktop, `RSPLAYER_DATA_DIR` handed over by the Kotlin `Application` on
  Android), picks a free port, exports `PORT`/`RSPLAYER_DESKTOP`, and runs
  `rsplayer::run_backend` as a tokio task while the webview shows
  `loading.html` until the port opens, then loads `http://localhost:<port>`.
- Desktop only: `tray` adds a system tray icon (Show, Previous,
  Play/Pause, Next, Quit). While `desktop_settings.close_to_tray` is on and
  the tray was created, the close request hides the window instead of
  shutting the backend down; the server mirrors the setting into
  `rsplayer::close_to_tray()` at startup and on every settings save.
  Tray creation is guarded with `catch_unwind` because libappindicator
  panics when its library is missing on Linux. `tauri-plugin-single-instance`
  runs before `setup`, which is where the desktop backend is started, so a
  second launch shows the running window and exits before it can open the
  same database.
- Desktop only: `tauri-plugin-window-state` saves the window's size,
  position and maximized state (not visibility or decorations) to
  `.window-state.json` in the app config dir on exit and when the window
  is hidden to the tray, and restores them when the window is created and
  when it is shown again from the tray (GTK maps a re-shown window at its
  initial size).
  `maximize_on_small_monitor` only runs when no saved state exists yet.
  The plugin is pinned to 2.4 because 2.5 requires tauri 2.12.
- Linux only, with `desktop_settings.custom_titlebar` on (the default):
  the window is undecorated and the web UI's nav bar is the title bar. An
  initialization script defines `window.__RSPLAYER_WINDOW__` (minimize /
  toggle maximize / close / `setDecorations` through Tauri's window
  commands); the UI shows its window buttons and marks the nav
  `data-tauri-drag-region` only when that object exists and the setting is
  on, so browsers and the other platforms are unaffected. The window starts
  undecorated and applies the saved setting (`rsplayer::custom_titlebar()`)
  once the backend port is open; toggling it in Settings switches the
  decorations live. `capabilities/titlebar.json` grants just those window
  permissions to the `http://localhost` origin the UI is served from; tao
  provides edge resizing for undecorated windows.
- `run_backend` is idempotent about process-global setup (crypto provider,
  logger) because "Restart RSPlayer" relaunches the executable on desktop but
  restarts the backend **in-process** on Android.
- Android: the backend is process-wide (`android::backend()`), started by
  the Tauri entry point or, with no activity (Android Auto, media button),
  by the Kotlin `PlaybackService` through `NativeBackend.start`. Whoever is
  first registers the app context with `ndk-context` (cpal's AAudio host and
  iroh's interface discovery need it): tao when an activity exists, else
  `android.rs`, which hands over to tao when an activity appears later. Media keys come from souvlaki on desktop; on Android
  the Kotlin `PlaybackService` (media3 `MediaSessionService`, foreground
  while playing) mirrors the backend over its WebSocket — the same JSON
  `UserCommand`/`StateChangeEvent` frames the web UI uses — and handles
  audio focus and the Wi-Fi multicast lock.
- Android has no ALSA: the server is built with `--no-default-features`, the
  platform profile is `TargetOs::Android` (cpal "System Default" device,
  software volume, shared Music folder pre-seeded as the library) and the
  output stream uses cpal's default buffer size.

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
- The sleep timer (`server/src/sleep_timer.rs`) is one more task raced in
  `run`'s `select!`: `PlayerCommand::SetSleepTimer`/`QuerySleepTimer` reach it
  through a handle in `CommandContext`, it broadcasts `SleepTimerEvent`
  (clients count down from `ends_at`), and when it fires it sends a normal
  `PlayerCommand::Pause` into the user command channel; the optional
  `sleep_timer_settings` steps then go to the `SystemCommand` channel
  (`SetFirmwarePower(false)`, `PowerOff`) once the pause has landed. In
  memory only.

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
  dirs; APE and SACD-ISO virtual tracks (`…#SACD_<n>`) via the custom readers
  in the metadata crate; cue sheet tracks (`…#CUE_<nn>`, see below) via
  `metadata::cue_bundle::open_track`. HTTP URLs are fetched once with `Icy-Metadata: 1`
  and `Range: bytes=0-`: a response carrying `icy-*` headers is a live radio
  stream (ICY title updates flow out as events, not seekable; stations with a
  metadata service, QuantumCast and Radiosphere, are polled by
  `NowPlayingPoller` instead and their ICY titles ignored); a host that
  honours byte ranges (podcast episodes, direct file links) becomes an
  `HttpRangeSource` — a seekable `MediaSource` that re-requests from the new
  offset on seek and reconnects at the current offset on dropped
  connections, so Symphonia reports a real duration and user seeks work;
  anything else plays as a plain non-seekable stream. Stream agents carry no
  global timeout (it would also bound body reads and cut long tracks).
  Instead, network sources (HTTP and remote-library) are opened and read on
  a `source-io` worker thread (`interruptible_source`): the playback thread
  waits for each open/read/seek in 100 ms slices, so Stop/Next interrupt a
  connecting or stalled stream at once, and an operation that takes over
  20 s fails as a stall and goes through the stream retry path.
  `stop_current_song` waits at most 3 s for the playback thread (it runs
  on the async command handler); each thread has its own stop flag, so one
  left behind stays stopped and does not report state once a newer one runs.
- **Remote libraries** (`metadata::remote_store`): S3-compatible storage
  configured in `MetadataStoreSettings.remote_libraries`. Nothing is mounted:
  the scanner lists objects (ListObjectsV2, via the sans-IO `rusty-s3`) and
  reads tags through presigned `GET` URLs wrapped in `HttpRangeSource`;
  playback opens the same way, with a fresh presigned URL per attempt (24 h
  expiry). A listing page gets three tries; a listing that still fails keeps
  the library's songs instead of deleting them and is named in the scan
  summary. Scan reads ride out network outages: chunk reads retry after a
  short pause, and after a failed file read the workers share an
  `OutageGate` (`metadata::outage_gate`). One worker checks whether the
  storage answers at all (`remote_store::is_reachable`) and, while it does
  not, keeps checking with growing pauses (up to 10 s apart, 5 min in all)
  while the others wait instead of failing the rest of the file list; only
  failures while the storage answers count against a file. Files left
  unread are reported as "not read (storage unreachable)". APE and SACD ISO are skipped (their readers need a file), and remote
  songs are not loudness-analysed (that would download them in full).
  `/music/<rs3 key>` answers with a 307 to the presigned URL, and secrets are
  masked (`SECRET_MASK`) in `GET /api/settings` and restored on POST.
- **Playback source**: `PlayerService` plays from a `PlaybackSource`
  (`api_models::playback_source`): `Queue`, `Radio(RadioStation)` or
  `Podcast { podcast_id, episode_id }`. Stations and episodes are played
  directly, never added to the queue. A `SourceNavigator`
  (`server/src/source_navigator.rs`, wired in the composition root) turns
  a source into the `Song` to decode and answers Next/Prev: radio cycles
  through `MetadataService::get_zap_stations` (favorites, then hand-added
  stations), podcasts walk the show's episodes by publish date
  (`PodcastService::adjacent_episode`); at the end of an episode the next
  newer unplayed one follows, a live stream that ends just stops. The
  source is persisted in `player_state` and broadcast as
  `PlaybackSourceEvent`. Leaving the queue saves its progress
  (`queue_resume_secs`) for `return_to_queue`; any queue play
  (`play_from_beginning`, `play_song`) switches back to `Queue`.
- **Resume hook**: before each track starts, `PlayerService` asks an optional
  `ResumePositionProvider` (the podcast service, wired in the composition
  root) for a start offset and arms `skip_to_time` with it. This is how a
  half-heard episode continues from any entry path (play button, next/prev,
  queue click) without the player knowing what a podcast is.
- **Failures**: only HTTP sources are retried, 3 times with 1/2/4 s
  backoff and one "reconnecting" notification per outage; the retry resumes
  at the position reached (`PlaybackContext::position_secs`, ignored by
  non-seekable radio), and a stream that played 30 s before failing gets a
  fresh set of retries. A song that still fails is skipped with a
  notification. `OutputUnavailable` (device missing, busy or rejecting the
  format) is never retried or skipped: it stops playback with `ERROR`, as do
  3 failed songs in a row.
- **Sample-format negotiation**: the device is opened at the source rate if
  it supports it, otherwise the resampler targets an integer multiple
  (cleanest ratio) or the closest supported rate; retry ladders handle
  drivers that reject advertised configs.
- **Opus** is decoded by `metadata::opus_bundle` (Symphonia ships no Opus
  decoder) on top of the pure-Rust `opus-rs` crate; Symphonia's Ogg, MKV and
  MP4 readers do the demuxing. Output is always 48 kHz f32. Symphonia exposes
  the Opus pre-skip only as `Track::delay`, so the decoder drops it itself
  from the first packet (pts <= 0) unless the container already marks a start
  trim; it also applies the header output gain. The decode loop resets the
  decoder after every seek (for all codecs), so a seek in Opus fades in over
  ~200 ms (no pre-roll).
- **Cue sheets** (`metadata::cue_bundle`): each track of a sheet is a
  virtual song keyed `{base}#CUE_{nn}`, `base` being the `.cue` file (or the
  audio file for a sheet embedded in its `CUESHEET` tag). The key has no
  offsets: `open_track` re-reads the sheet, so keys survive fixed sheets.
  The scanner parses every `.cue` it lists; a sheet whose audio is found
  claims that file, which is then not a song of its own (a leftover sheet
  without audio, or a second sheet for the same file, is ignored).
  Playback wraps the file's reader in `CueTrackReader` (seeks to the track,
  stops at its end, pts rebased to 0) and the decoder in `CueTrimDecoder`,
  which cuts the boundary packets (a FLAC block is ~93 ms, an APE frame
  ~1.7 s) to the exact sample; Symphonia's FLAC decoder ignores packet
  trims. Track boundaries are `INDEX 01`, so a pregap plays at the end of the
  previous track. Loudness analysis opens tracks the same way. In browser
  playback, `/music/<cue key>` serves the track as a WAV decoded per range
  request (`cue_bundle::wav`). Tracks of one file are separate songs to the
  player, so the usual track change happens between them.
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
| `songs` | `Song` JSON keyed by library-relative path; songs from a remote library are keyed `rs3:<library id>/<path below prefix>` |
| `albums` | Albums keyed by `mb\|<MusicBrainz album id>`, else normalized `folder\|album artist tag\|album` (disc folders folded in, track artist not used; see `album_identity.rs`). Rebuilt from songs once by the `album_identity_v2` migration |
| `play_statistics` | Play/skip/like counters per song key; `radio_uuid_*` keys hold the liked radio-browser stations |
| `radio_stations` | Stations added by hand in the UI (`RadioStation` JSON keyed by a generated uuid), plus the details of liked radio-browser stations (keyed `fav_{stationuuid}`, `radio_browser_uuid` set) so Next/Prev can switch between favorites |
| `loudness` | Integrated LUFS per song key |
| `scan_state` | `running` marker while a library scan runs (a leftover one at startup means the scan was interrupted) and the last scan's `ScanSummary` JSON, sent to clients on `QueryScanStatus` |
| `queue`, `queue_status`, `queue_random_history` | Queue items (insertion-ordered ids), current position/mode, random-mode history |
| `playlist`, `playlist_list` | Saved playlist items (`{name}_{index}`) and headers |
| `player_state` | Pause flag + last position for resume-on-restart, current `PlaybackSource`, queue progress saved when switching to radio/podcast |
| `podcasts` | Subscribed feeds (`Podcast` JSON keyed by feed-URL hash) incl. refresh validators (`ETag`/`Last-Modified`) |
| `podcast_episodes` | `Episode` JSON keyed `{podcast_id}/{inverted publish ts}/{episode_id}` — a prefix scan is newest-first; carries `position_secs`/`played` |
| `podcast_episode_index` | `id:{episode_id}` and `url:{audio_url}` → episode key, for lookups from playback events |
| `multiroom` | The iroh endpoint secret key |
| `scrobble_queue` | Scrobbles that could not be sent yet, keyed `{lb\|lfm}/{seq}` (oldest first per service, at most 1000 each); flushed after the next successful call and every minute with backoff |

The settings blob round-trips **whole** through `GET/POST /api/settings`;
there is no merge. A stale client posting an old schema drops newer sections
— that is why every newer field has `#[serde(default)]` and serialized names
are frozen (the `alsa_*` settings names predate cross-platform support and
now configure cpal everywhere).

Secrets do not leave the server: `GET /api/settings` masks the S3 secret
keys and the scrobbling token, API secret and session key, and `POST`
restores masked values from the stored settings. The Last.fm session key
and user name are server-managed (set by the connect flow, cleared by
disconnect or an invalid-session answer) and always kept from the stored
settings on `POST`.

### Scrobbling rules

`crates/scrobble/src/machine.rs` decides from the event stream alone: a
track starts with `CurrentSongEvent` + `PlaybackStateEvent(PLAYING)`
(a lone `CurrentSongEvent` is a query or like re-broadcast). "Now playing"
goes out on every start and resume; one scrobble per play once the listened
time (sum of 1-2 s progress steps while playing, so seeks forward do not
count) reaches half the track or 4 minutes, whichever is less; tracks of
30 s or less never count. The same song starting again near 0 after its
last play got past 5 s is a new play (repeat one, restart), otherwise a
resume. Only library songs played from the queue with artist and title are
reported: no radio, no podcast episodes (also when queued), nothing while
the instance is a multiroom follower (`MultiroomGroupEvent` role, plus the
`multiroom_follower_active` flag) or in browser playback mode (the server
sees no progress there). Not started in demo mode.

## HTTP / WebSocket Server

- axum; web UI embedded via rust-embed (debug builds read `dist/` from disk).
- `/api/ws`: commands in, every `StateChangeEvent` broadcast out to all
  clients.
- `/api/settings` (whole-struct GET/POST + validation), `/api/artwork/<id>`,
  range-capable audio streaming for local-browser playback, and an optional
  HTTPS listener.
- `/rest/*`: the Subsonic / OpenSubsonic API (`crates/server/src/subsonic/`)
  for third-party music apps, over the same repositories as the browse API.
  Always mounted, answers only while enabled with a password
  (`SubsonicSettings`); single user, password or `md5(password + salt)`
  token auth. Ids are type prefix + base64url of the library key. Library
  work runs on blocking threads; `stream`/`download` send the original file
  in full or by range (no transcoding, cue tracks as WAV, remote songs
  redirected), merged after the global compression layer so audio is never
  gzipped. See [Subsonic API](subsonic.md).
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
