---
name: Multiroom report (beta)
about: Report a synchronization, grouping, or discovery problem with multiroom playback
title: "[multiroom] "
labels: multiroom
---

<!-- Thanks for testing the multiroom beta! The four items below diagnose
almost every sync problem — please fill them in for EACH device involved. -->

## What happened

<!-- What you did, what you expected, what you heard/saw instead.
E.g. "kitchen room is ~200ms behind", "join fails from the leader side",
"crackling on the follower after ~30s". -->

## Devices

| | Leader | Follower(s) |
|---|---|---|
| Hardware (machine, DAC/output device) | | |
| OS / distribution | | |
| Audio stack (PipeWire / plain ALSA / direct `hw:` device) | | |
| Network (wired / Wi-Fi; same switch/AP?) | | |
| RSPlayer version | | |

## Firewall

<!-- Is a firewall active on any device? Is UDP port 47800 open (see the
multiroom docs)? Does the problem change with the firewall temporarily off? -->

## Logs

<!-- From the misbehaving device(s), captured with:

    RUST_LOG=info,sync=debug,playback=debug

Please include the lines around the problem, ideally covering:
- "Multiroom endpoint bound on ..." (startup)
- "Multiroom sink opened: ... device latency ..." (follower session start)
- "Multiroom sink scheduling error ..." (periodic sync health)
- "clock offset ... rtt ..." (clock sync quality)
-->

```
paste logs here
```
