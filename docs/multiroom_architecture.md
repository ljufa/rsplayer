# Multiroom Architecture (Developer Notes)

Internals of the multiroom sync feature: code layout, wire protocol, timing model, and the reasoning behind the non-obvious decisions. User-facing documentation lives in [Multiroom Playback](multiroom.md).

## Code Map

| Location | Responsibility |
|----------|----------------|
| `crates/sync/src/lib.rs` | `SyncService`: role state machine (Idle ⇄ Leader / Follower), peer map, group commands, main `select!` loop |
| `crates/sync/src/endpoint.rs` | iroh endpoint construction, persisted identity, mDNS registration |
| `crates/sync/src/protocol.rs` | Wire messages (postcard), stream framing, ALPN, size limits |
| `crates/sync/src/clock.rs` | NTP-style offset estimator, `ClockState` (lock-free atomics) |
| `crates/sync/src/leader.rs` | Tee ingestion → timestamped chunks → broadcast fan-out; per-follower audio writer; clock responder |
| `crates/sync/src/follower.rs` | Control-stream driver, clock pinger, per-session audio receivers, sink lifecycle |
| `crates/playback/src/rsp/tee.rs` | `SyncTee`/`TeeSession`: PCM copy-out from the decode loop; `MonoClock` |
| `crates/playback/src/rsp/sync_sink.rs` | `SyncSink`: scheduled playback thread with drift correction |
| `crates/playback/src/rsp/alsa_output.rs` | `prefill_silence_ms`, `playback_lag_micros` (playback-position sensor) |
| `crates/playback/src/rsp/symphonia.rs` | Tee hook points in the decode loop (session lifecycle) |
| `crates/server/src/composition_root.rs` | Builds `SyncTee` + channels when enabled, wires `SyncDeps` |
| `crates/api_models/src/{settings,common,state}.rs` | `MultiroomSettings`, `MultiroomCommand`, UI events |
| `web-ui/src/page/player.rs` | Peer panel (leader) and follower banner |

Dependency direction: `server → sync → playback`. The tee and sink live in `playback` because they need `AlsaOutput` (crate-private); `sync` consumes them. `crates/wire` is unrelated (USB front-panel protocol).

## Identity, Discovery, Dialing

- Each instance owns an ed25519 **secret key persisted in the fjall keyspace `multiroom`** (key `secret_key`) — the resulting iroh `EndpointId` is the device's stable identity across restarts and IP changes. It is deliberately *not* stored in `Settings`: the whole settings struct round-trips through `GET/POST /api/settings` to every browser.
- The endpoint binds with `presets::Minimal` + `RelayMode::Disabled` — **LAN-only**, no relay servers, no `dns.iroh.link`. ALPN: `rsplayer/sync/1`.
- Transport config tightens dead-connection detection: **8 s idle timeout, 2 s keep-alive** (iroh defaults: 30 s / 5 s). A killed follower sends no `CONNECTION_CLOSE`, so the leader's control-stream read only fails at the idle timeout — with the default that left the follower in the group (checkbox active) for ~30 s after its mDNS record had already expired. The effective timeout is the minimum of both peers' values, so mixed versions interoperate.
- The endpoint binds **fixed UDP port 47800** (`SYNC_UDP_PORT`) so device firewalls can be opened for it — on a random ephemeral port, an inbound group-initiation dial is silently dropped by default-deny firewalls (observed on Fedora's `public` zone: joining a device failed until that device had dialed out once and created conntrack state). If the port is taken (second instance on one host), it falls back to a random port with a warning.
- The **room name travels as mDNS user-data** (`user_data_for_address_lookup`), so the peer list renders names without connecting.
- Every mDNS announcement's addresses are cached per peer (`Peer.discovered_addr`). Dial order: manual address → last announced addresses → bare `EndpointId` (active lookup). Rationale: passive announcements are far more reliable than active mDNS queries at connect time; bare-id dialing was observed to fail intermittently while the peer was plainly visible. Dials are bounded by a 12 s timeout with a user-visible error.
- `MultiroomCommand::AddManualPeer("endpoint_id[@ip:port]")` exists as a fallback for networks without working multicast (also used to group two instances on one host in tests).

## Roles and Group Protocol

One tokio task owns all mutable state (`Service`); connection tasks communicate with it via an internal `Event` mpsc. A `busy: Arc<AtomicBool>` lets incoming-handshake tasks reject a second leader without consulting the main loop (benign races resolve there).

Handshake on the control stream (bidi, opened by the leader):

```
leader → Hello { protocol_version, leader_name }
follower → HelloAck { protocol_version, follower_name }   (rejects if busy / version mismatch)
```

After that the control stream carries `GroupState`, `StreamStart/StreamStop`, `TimelineCorrection`, `SongProgress`, keep-alive pings, and (follower→leader) `LeaveGroup`. Membership teardown is connection-close in both directions — every path (leave, remove, crash, network loss) converges on "connection died → revert to Idle".

While grouped, the follower's server rejects local transport commands via a shared `AtomicBool` checked in `player_commands.rs`; volume commands are untouched (per-room volume).

## Wire Protocol

- **Serialization: postcard** everywhere; streams frame messages with a `u32` LE length prefix. Size caps: 256 KiB control, 4 MiB audio frames.
- **Control = one bidirectional stream** (reliable, ordered — exactly what session state needs).
- **Audio = one reliable *uni* stream per session** (leader → follower). With 500–1000 ms of scheduling headroom, a LAN retransmit never threatens a deadline, QUIC flow control provides free backpressure, and flush-on-seek/stop is simply "drop the stream". Datagrams were rejected for audio: they'd require reinventing ordering/loss handling for zero latency benefit at this buffer depth.
- **Clock probes = QUIC datagrams**: timing packets must never queue behind retransmitted audio bytes (head-of-line blocking would corrupt RTT measurements).
- `AudioChunk { session_id, seq, play_at_micros, frames, payload }` — payload is interleaved **f32 LE at the source sample rate**.

Why f32 at source rate (and not the leader's output format): the tee is taken **pre-EQ, pre-resample, pre-volume** in the decode loop. Anything later would bake the leader's room EQ and device sample rate into every room. Followers run the received PCM through their own full output pipeline (rate conversion, EQ, VU, software volume), so per-room correction keeps working. Loudness-normalization gain is the one leader-side value that must be carried along (`StreamStart.gain_db_hundredths`) because it is normally applied inside the leader's DSP chain. Cost: 2.8 Mbit/s per follower at 44.1 kHz stereo; a 16-bit wire format is the planned optimization for weak devices (Pi Zero).

## The Timing Model

Everything hangs off one invariant: **chunk timestamps are sample counts, not send times.**

- `MonoClock` (process-wide monotonic µs; defined in `playback::rsp::tee` because both the playback thread and the sync crate must share a single epoch per process).
- At session start the leader computes `epoch` = the moment frame 0 reaches its own DAC. Every chunk then carries `play_at = epoch + first_frame / rate`. `first_frame` is counted **on the playback thread**, so a chunk dropped anywhere downstream shifts nothing — followers hear a gap, never a permanent offset.
- The leader delays its own output by the sync buffer: `AlsaOutput::prefill_silence_ms` pushes silence through the normal writer path (so resampler/EQ state stays consistent), and `epoch` is then *measured* as `now + playback_lag` rather than assumed.

### Clock sync (`clock.rs`)

Followers send `Ping { id, t1 }` datagrams (burst of 10 × 100 ms on join, then one per 2 s); the leader stamps `t2`/`t3`. Classic NTP math yields offset and RTT per exchange. The estimator keeps a 30-sample window, takes the **median offset of the 5 lowest-RTT samples** (low RTT ⇒ least asymmetry error), and smooths with EWMA α = 0.1 into a lock-free `ClockState`. Audio is held until ≥ 5 samples. Loopback quality: offset stable to ~1 ms at ~2 ms RTT.

### The playback-position sensor (`playback_lag_micros`)

`lag = ring backlog + device latency` — the time until a sample pushed *now* is audible. Device latency comes from cpal's `OutputCallbackInfo` timestamps, with two hard-won corrections:

1. **Aging**: the reported latency is only valid at the callback instant; between callbacks the device buffer drains, so the stored value is reduced by its age. Without this the sensor sawtooths by a full device buffer (93 ms at the default `Fixed(4096)`).
2. **EWMA (α = 1/8) + freeze after 64 callbacks**: PipeWire's ALSA plugin reports latency that *ramps up* for seconds while its buffers fill, HDA Intel PCH reports jumpy values, and PipeWire sometimes reports no timestamps at all (sensor degrades to ring-only). Sync needs a stable reference more than a live one, so the estimate is frozen once warmed up.

### Closing the loop

Two independent clocks drift: the leader's DAC vs. `MonoClock`, and each follower's DAC vs. its `MonoClock`.

- **Leader**: every 2 s the playback thread compares where its DAC actually is (`now + lag` for the last teed frame) against the nominal timeline and publishes the difference as `TimelineCorrection` (averaged over the interval to cancel sensor sawtooth). Followers add it to chunk timestamps — they track the leader's *real* output, not the theoretical timeline.
- **Follower** (`sync_sink.rs`): per chunk, `error = (now + lag) − play_at` for the next sample to be pushed. The response is deliberately conservative:
  - error EWMA (α = 1/8) — raw per-chunk values are too noisy;
  - **slew** with hysteresis (start > 5 ms, stop < 1.5 ms): **time-stretch** each chunk by at most 8 frames per 1024 (~0.8 %, ≈14 cents — inaudible) via linear resampling that preserves the chunk's first/last frames, so consecutive chunks stay waveform-continuous. Each applied stretch is folded into the error EWMA immediately; the filter lags ~8 chunks and would otherwise overshoot and oscillate;
  - **hard correction** > 20 ms: full one-shot trim/silence-insert (with a 256-frame fade-in over the splice), but only after **8 consecutive same-direction** out-of-threshold chunks, followed by a 2 s cooldown;
  - **warmup**: after the initial alignment, *all* corrections are suppressed for 7 s (latency reports still ramping), then at most one clean correction locks the stream.

The initial alignment itself is just the same mechanism with no filter history: the first chunk's error is typically −(sync buffer), corrected by leading with silence — pushing blocks on the full ring, which self-paces it.

The persistence/cooldown/warmup complexity exists because of real hardware: without it, jumpy sensors caused correction ping-pong (audible breakage on HDA Intel PCH). The slew originally spliced frames in/out directly; with the error EWMA hovering a few ms around a single 2 ms deadband that produced a waveform discontinuity ~10×/s — audible as periodic crackling on every driver — hence the time-stretch + hysteresis design.

### Session lifecycle

`play_file` owns an optional `TeeSession` (drop guard ⇒ every exit path notifies followers):

- **Track start** → output opened → prefill → `SessionStart` (new session id).
- **Seek** → session ended + local output dropped (ring discarded) → next packet opens a fresh, re-timed session. Followers flush on `StreamStop`, so a seek cuts everywhere at once.
- **Track end** → the session drops only *after* the leader's ring drains, and the follower's channel disconnect makes its sink **drain** (play the scheduled tail) — both sides finish together. Stop, by contrast, **flushes** (immediate). This drain-vs-flush distinction is why `SyncSink` has both `join()` and `stop()`.
- **Mid-track join**, two cooperating mechanisms: (1) when the *first* follower joins during playback, the decode loop starts a session at the current position with `epoch = now + playback_lag` — no prefill and no local interruption, because the leader's ring backlog already provides the scheduling headroom; (2) session metadata (`SessionInfo`) rides inside **every** broadcast chunk, so any *additional* follower (or one recovering from lag) picks an already-running session up at the next chunk.

## Fan-out and Backpressure

The playback thread must never block on the network: `SyncTee` sends via `try_send` into a bounded channel; on overflow chunks are dropped (timeline unaffected — see `first_frame`). The ingestion task republishes on a `tokio::broadcast` (cap 512); each follower's writer task subscribes. A slow follower lags its own receiver and gets a gap; repeated lag is its problem alone — nothing propagates back to local playback or other followers.

## Threading

- Playback thread (existing) — decode + tee + local output; realtime priority per `player_threads_priority`.
- `multiroom-sink` thread per active session on followers — same priority setting. **Both use the configured priority on single-core machines too**; the old `ThreadPriority::Min`-on-single-core behavior let web-UI traffic starve audio on a Pi Zero (broke playback even without multiroom).
- Everything else (QUIC, control, clock, fan-out) is tokio tasks.

## Performance and Quality Impact

What the feature costs in each state — the design goal is that you only ever pay for what is actively in use:

| State | Audio quality | CPU / network | Latency |
|-------|--------------|---------------|---------|
| Disabled | untouched | zero | untouched |
| Enabled, not grouped | untouched | negligible | untouched |
| Grouped, leader | untouched locally | small copy per chunk + QUIC/2.8 Mbit/s per follower | +`buffer_ms` on play/seek |
| Grouped, follower | altered only while correcting | QUIC decrypt + stream (~45 % on Pi Zero, trivial on desktop) | scheduled by leader |

- **Disabled**: the gate is at composition time (`composition_root.rs`) — no `SyncTee` is constructed, the sync service future is never started, so there is no endpoint, no UDP socket, no mDNS traffic, no extra threads. The decode loop's tee hooks are `None`-checks. Playback is bit-for-bit what it was before the feature existed.
- **Enabled, not grouped**: what runs is an idle UDP socket, periodic mDNS announcements, and one parked ingestion task. The audio hot path pays one atomic load per packet (`SyncTee::is_active`). No audio is copied or serialized.
- **Grouped, as leader**: local output is *bit-identical* to ungrouped playback — the tee copies decoded f32 pre-EQ/pre-resample and the local pipeline processes the same samples either way. Costs: one interleave copy per chunk on the playback thread (bounded, `try_send`, can never block — see Fan-out), QUIC encryption + 2.8 Mbit/s per follower on the tokio runtime, and `buffer_ms` (default 500 ms) of self-delay on session start so followers can align — a latency cost on play/seek reaction, not a quality cost.
- **Grouped, as follower**: the only state where samples can be altered: the drift corrector time-stretches by ≤ 0.8 % *while actively slewing* (hysteresis keeps it off in steady state) and hard corrections skip/fade once on join or after a stall. Otherwise received PCM passes through the follower's normal output pipeline (own EQ, VU, volume) unchanged. CPU is dominated by QUIC crypto + the f32 stream — ~45 % on a Pi Zero, trivial on desktop-class hardware.
- **Caveat**: while grouped, every track starts a fresh session with its own prefill, so transitions are **not gapless** (known leftover). Ungrouped playback — enabled or not — keeps normal gapless behavior.

## Decisions Summary

| Decision | Alternative rejected | Why |
|----------|---------------------|-----|
| iroh 1.0 | hand-rolled quinn + mdns-sd | discovery + identity + encryption in one coherent layer; wire-protocol stability guarantee across versions |
| Stream decoded PCM | "play file X at T" + local decode | sample-identical output everywhere; no library access needed; radio works; decoder differences can't drift |
| f32 @ source rate, pre-DSP tee | post-EQ tee at device rate | per-room EQ/resampling must keep working; leader's device config must not leak into other rooms |
| Sample-count timestamps | send-time timestamps | immune to network jitter and chunk drops |
| Reliable uni stream for audio | QUIC datagrams | buffer depth makes retransmits free; ordering/loss handling for free; flush = drop stream |
| Leader publishes DAC-position corrections | followers slave to nominal timeline | leader's DAC is a drifting clock too; a nominal timeline would walk away from what the leader actually plays |
| Measured epoch + frozen latency sensor | trusting driver latency live | drivers lie: ramping (PipeWire), jumpy (HDA), or silent (PipeWire ALSA plugin) |
| Persistence + cooldown + warmup before corrections | react per chunk | sensor noise must not cause audible skips; crackle during warmup was audible |

## Prior Art and Theory References

Most of the building blocks are established concepts; the names in the code are chosen to point at them.

- **Tee** (`SyncTee`, `tee.rs`): after the Unix `tee(1)` command — itself named for the T-shaped plumbing fitting — which copies a stream to the side without disturbing the main pipe. The same concept/name appears as GStreamer's `tee` element and Go's `io.TeeReader`. The plumbing metaphor carries the design invariant: a tee observes the flow, it must never block it (hence `try_send` + drop-on-overflow).
- **Clock synchronization** (`clock.rs`): classic **two-way time transfer** as used by NTP — offset = ((t2−t1)+(t3−t4))/2, uncertainty bounded by RTT/2. See RFC 5905 (NTPv4) and D. L. Mills, *Computer Network Time Synchronization*; the "keep a window, prefer low-RTT samples" selection mirrors NTP's clock-filter algorithm, and the RTT-bounds-the-error argument is Cristian's algorithm (F. Cristian, *Probabilistic Clock Synchronization*, 1989). The professional-audio equivalent is PTP/IEEE 1588 (used by AES67/Dante), which needs hardware timestamping we don't have — low-RTT filtering on a LAN gets within ~1 ms, which is enough below the audibility threshold for inter-room alignment.
- **Sensor smoothing**: the EWMA filters on latency reports and scheduling error are first-order IIR low-pass filters (exponential smoothing) — the standard cheap noise filter with O(1) state.
- **Slew hysteresis**: the start/stop double threshold is a Schmitt trigger — the textbook cure for a noisy signal chattering across a single threshold (which here produced a correction toggling on/off chunk by chunk).
- **Drift correction by micro-resampling**: playing marginally faster/slower to servo a buffer level is how PulseAudio's `module-combine-sink` keeps multiple sinks aligned (adaptive resampling) and how Snapcast implements "soft sync"; the general DSP topic is asynchronous sample-rate conversion (ASRC). Our linear-interpolation stretch is the simplest ASRC — at ≤ 0.8 % ratio its distortion is far below audibility, so nothing fancier is warranted.
- **System shape**: the leader/follower buffered-timestamp architecture (decode once, distribute timestamped PCM, followers schedule against a synced clock) is the same shape as **Snapcast** — the main departures are QUIC/iroh transport with encryption and identity, pre-DSP f32 tee for per-room EQ, and the driver-reported-latency feedback loop instead of fixed per-device offsets.
- The follower's correction loop as a whole is a feedback controller with a deadband: measure error, apply bounded proportional correction, suppress response to noise — closer to control-engineering common sense (hysteresis + slew limiting) than to any single named algorithm.

## Testing

- Unit tests: clock estimator (synthetic exchanges incl. asymmetric jitter), protocol round-trips.
- Two instances on one machine: run from **separate working directories** (the fjall DB is cwd-relative) with different `PORT`s, both on the PipeWire default sink (mixes clients; raw `hw:` devices can't be opened twice). Group via the UI or raw WS commands.
- Sync measurement: play a click-track WAV, record the mixed output (`pw-record -P '{ stream.capture.sink=true }' out.wav`), measure click burst *width* — width minus source click length ≈ inter-room misalignment. Local baseline: ~2 ms.
- Beware the resume-position feature when scripting tests: `Play` seeks to the last saved position, which is the end of the track if the previous test run played it out.
- Logs: `RUST_LOG=info,sync=debug,playback=debug` shows session lifecycle, clock offset/RTT, measured device latency, and every correction.
