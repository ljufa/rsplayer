# Multiroom Playback (beta)

RSPlayer can play the same music on several devices at once, synchronized to a few milliseconds — no external software (Snapcast, Roon, etc.) required. Every RSPlayer instance on your network can discover the others automatically, and any of them can stream its playback to the rest.

Typical setup: a Raspberry Pi with a USB DAC in the living room, another in the kitchen, and a desktop in the office — group them from the web UI and they all play in sync.

> **Beta status.** Multiroom works well on wired LANs and the hardware it has been developed against (desktop Linux with PipeWire, Raspberry Pi with USB DACs). Synchronization quality, however, depends on things outside RSPlayer's control — audio-driver latency reporting, Wi-Fi access points, device firewalls — and the variety out there is huge. If sync misbehaves on your setup, please [open an issue](https://github.com/ljufa/rsplayer/issues/new?template=multiroom_report.md) and include: your OS and audio stack (PipeWire / plain ALSA / direct `hw:` device), the DAC or output device, wired or Wi-Fi, and a log captured with `RUST_LOG=info,sync=debug,playback=debug` — the log contains the timing measurements needed to diagnose sync problems.

## Requirements

- RSPlayer installed on each device (any mix of Linux, macOS, and Windows works — the sync protocol is identical on all platforms).
- All devices on the same local network.
- The network must allow **UDP traffic** between the devices and **multicast (mDNS)** for automatic discovery. Home routers allow this by default; if a device firewall is active, allow inbound **UDP port 47800** (RSPlayer's fixed sync port) and mDNS (UDP port 5353).

  On firewalld-based systems (Fedora, RHEL):

  ```
  sudo firewall-cmd --add-service=mdns --add-port=47800/udp --permanent
  sudo firewall-cmd --reload
  ```

  With ufw (Ubuntu, Debian-based):

  ```
  sudo ufw allow 47800/udp comment 'RSPlayer multiroom sync'
  sudo ufw allow 5353/udp comment 'mDNS discovery'
  ```

  With raw iptables:

  ```
  sudo iptables -A INPUT -p udp --dport 47800 -j ACCEPT
  sudo iptables -A INPUT -p udp --dport 5353 -d 224.0.0.251 -j ACCEPT
  ```

  and make it persistent with your distribution's mechanism (e.g. `iptables-save` + the `iptables-persistent` package on Debian/Ubuntu).

## Setup

On **each** device that should take part:

1. Open **Settings → Multiroom**.
2. Enable **multiroom (synchronized playback)**.
3. Give the device a **room name** (e.g. "Living room", "Kitchen") — this is the name other devices will see.
4. Restart RSPlayer (the settings page offers this after saving).

Each device generates a permanent cryptographic identity on first start, so groups keep working across restarts and IP address changes.

## Usage

### Grouping rooms

On the device that should be the **leader** (the one whose queue and playback you want everywhere):

1. Open the **player page**. Once other multiroom-enabled devices are discovered, a **Multiroom** panel appears below the playback controls, listing each room with an online indicator and a toggle.
2. Toggle a room **on** to add it to the group. It becomes a **follower** immediately.
3. Play music as usual — every track you play on the leader now also plays, synchronized, in all grouped rooms. Followers even join cleanly in the middle of a running track.

### Follower behavior

While a device is a grouped follower:

- Its player page shows a banner — *"Grouped with '\<leader\>' — playback is controlled by the leader"* — with a **Leave group** button.
- Its own transport controls (play/pause/next/seek) are disabled; commands go through the leader.
- **Volume stays local**: each room keeps its own volume control, and its own DSP/EQ settings apply to the received stream — per-room correction works as usual.
- The current song and progress from the leader are mirrored in the follower's UI.

### Ungrouping

- From the **leader**: toggle the room off in the Multiroom panel.
- From the **follower**: click **Leave group** on its banner.
- If the connection between devices is lost, the follower automatically returns to normal standalone operation.

## Settings Reference

| Setting | Default | Description |
|---------|---------|-------------|
| Enable multiroom | off | Turns the feature on and makes the device discoverable. Requires a restart. |
| Room name | hostname | Human-readable name shown on other devices. |
| Sync buffer (ms) | 750 | How far ahead audio is scheduled. The leader delays its own output by this amount and streams that far ahead, giving followers headroom to receive and align the audio. Raise it (1000–1500 ms) if a follower on weak Wi-Fi or a slow CPU gets dropouts; lower it for a snappier reaction to play/seek. It does **not** shift rooms relative to each other. |
| Output latency trim (ms) | 0 | Per-room constant offset, applied when this device plays as a follower. Positive = this room plays later. Use it only if a room consistently sounds ahead/behind after everything else is working — typically a device whose driver misreports its latency. |

## How It Works

For the curious — this is what happens under the hood. (Developers: see [Multiroom Architecture](multiroom_architecture.md) for protocol and implementation details.)

```
Leader                                        Follower(s)
──────                                        ───────────
decode (Symphonia)                            QUIC endpoint (iroh)
  │ tee: raw PCM (f32, source rate)             │ control stream: session start/stop,
  ├──► local output, delayed by                 │   metadata, timeline corrections
  │    the sync buffer                          │ audio stream: timestamped PCM chunks
  └──► QUIC streams to each follower ──────►    │ clock sync: ping/pong datagrams
                                                └──► scheduled playback through the
                                                     normal output pipeline
```

### Discovery and transport

Devices find each other with **mDNS** announcements (the same zero-config mechanism AirPlay and Chromecast use) and talk over **[iroh](https://www.iroh.computer/)** — QUIC connections dialed by cryptographic device identity rather than IP address. All multiroom traffic is end-to-end encrypted, stays on your LAN, and never touches any external server. Room names travel inside the mDNS announcement, so the device list is populated without connecting.

### Audio distribution

The leader decodes each track **once** and duplicates the raw PCM (at the source sample rate) before any of its local processing. Grouped followers receive it as 16-bit PCM in timestamped chunks over a reliable QUIC stream and run it through their **own** output pipeline — device-rate resampling, parametric EQ, volume, visualizer all apply per room. Loudness-normalization gain computed by the leader is carried along and applied by each follower. Bandwidth is roughly 1.4 Mbit/s per follower for 44.1 kHz stereo (6 Mbit/s at 192 kHz) — trivial for wired networks and fine for healthy Wi-Fi. 16-bit sources (CDs, most FLAC) cross the wire losslessly; higher bit depths are transparently reduced to 16 bits for transmission on follower rooms only — the leader's local playback is untouched.

### Staying in sync

Three mechanisms hold the group together:

1. **Clock synchronization.** Each follower continuously measures its clock offset to the leader with NTP-style probe packets (a burst on join, then every 2 s), keeping only the cleanest low-round-trip samples. On a LAN this pins the shared clock to well under a millisecond.
2. **Scheduled playback.** Every audio chunk carries the exact time it must reach the speakers. The leader delays its own output by the sync buffer (using measured ring-buffer and device latency), so "now + buffer" means the same instant everywhere.
3. **Drift correction.** Sound cards run on independent crystals that drift apart by tens of milliseconds per hour. Each follower measures where its DAC actually is versus where it should be — including the device latency reported by the audio driver — and corrects continuously by time-stretching the audio a fraction of a percent (inaudible). The leader likewise publishes where its own DAC really is every 2 s, so followers track the leader's *actual* output, not a theoretical timeline. Large errors (a network stall, joining mid-track) are fixed with a single one-shot correction instead.

In practice, rooms align to **single-digit milliseconds** and stay locked for arbitrarily long sessions.

## Limitations

- **DSD tracks are not streamed** to the group — they play on the leader only (DSD bitstreams can't pass the PCM processing chain).
- **No gapless playback while grouped**: each track starts a new synchronized session, adding a sync-buffer-sized pause between tracks.
- **Radio streams** work, but a network stall on the leader is heard as the same brief silence in every room.
- **Low-power devices** (e.g. Raspberry Pi Zero) spend noticeable CPU on receiving the encrypted stream (~40–50% of a Pi Zero core). They work as followers, but raise the sync buffer and ring buffer, and avoid heavy UI use on that device during playback.
- The leader's queue is the source of truth; followers can't edit the shared queue from their own UI (planned).

## Troubleshooting

**A room doesn't appear in the list.**
Multiroom must be enabled (and the service restarted) on *both* devices. Check that your network allows multicast — some Wi-Fi access points have "AP/client isolation" that blocks device-to-device traffic entirely. As a diagnostic, each device logs its identity on startup: `Multiroom endpoint bound. This instance's endpoint id: …`.

**A room is listed but joining fails.**
You'll get a "connection timed out" notification after ~12 s. Usually a firewall blocking UDP on the other device, or the device just changed IP address — wait a few seconds for the next announcement and try again. A telltale firewall symptom: joining a device fails until *that* device initiates a connection once, then works. Open UDP port 47800 on it (see Requirements). If the log on a device says `Failed to bind multiroom UDP port 47800`, another program (or a second RSPlayer instance) holds the port and that device fell back to a random one — a firewall exception for 47800 won't cover it.

**Rooms are audibly offset.**
Check the follower's log for `Multiroom sink opened: … device latency …`. If it says `not reported`, that audio driver doesn't provide latency timestamps and its output delay can't be compensated automatically — set **Output latency trim** on the room that's ahead (positive values delay it) until it locks in.

**Crackling or dropouts on a follower.**
Usually CPU starvation or network jitter. Raise the **sync buffer** on the leader and the **ring buffer size** (Settings → Playback → Advanced) on the affected follower. On single-board computers also raise **player threads priority** so audio outranks the web UI. The follower log prints its scheduling health periodically: `Multiroom sink scheduling error …µs`.

**Where to look in the logs.**
Run with `RUST_LOG=info,sync=debug,playback=debug` for the full picture: session lifecycle (`StreamStart`/`StreamStop`), clock quality (`clock offset …µs, rtt …µs`), device latency, and every sync correction are all logged.
