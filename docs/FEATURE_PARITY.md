# Feature Parity Comparison: rsplayer vs. Others

This document provides a feature comparison between **rsplayer** and other popular music playback solutions as of January 2026.

| Feature Category | Feature | **rsplayer** | **Volumio** | **Roon** | **MPD** | **Navidrome** | **LMS** |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Core** | **Architecture** | Headless Service + Web UI | OS / Headless + Web UI | Core + Remote + RAAT | Client-Server (Daemon) | Client-Server (Web/API) | Server + Squeezebox Clients |
| | **Target Hardware** | SBCs (RPi), x86 Linux | SBCs, x86, Streamers | PC/Mac/Linux, Roon Ready | Linux, BSD, macOS, Win | PC, Server, NAS, SBCs | PC, NAS, SBCs |
| | **License / Cost** | Open Source / **Free** | Freemium | Subscription ($$$) | Open Source / **Free** | Open Source / **Free** | Open Source / **Free** |
| | **Setup Complexity** | **Low** (single binary + script) | Low (OS image) | Medium (Core + Remotes) | **High** (daemon + separate client) | Low (Docker/binary) | Medium (Perl server + clients) |
| **Playback** | **Audio Engine** | Rust (Symphonia) | MPD (Customized) | RAAT | C++ (Native) | Go + FFmpeg | Perl + FLAC/FFmpeg |
| | **Codecs** | **Comprehensive** | Comprehensive + DSD | Comprehensive + MQA | **Very Comprehensive** | Comprehensive | **Very Comprehensive** |
| | **Audio Output** | ALSA, DSD Native/DoP | ALSA, I2S, USB | RAAT, AirPlay, USB | ALSA, Pulse, PipeWire | Web Browser, API Stream | Squeezebox, AirPlay, Chromecast |
| | **Streaming Services**| **None** (Planned) | Spotify, Tidal, Qobuz | Tidal, Qobuz, KKBOX | Limited (via plugins) | **None** (Self-hosted) | Spotify, Deezer (via plugins) |
| | **Internet Radio** | **Yes** | Yes | Yes | Yes | Yes | **Yes** |
| **Library & Data** | **Library Source** | Local Files (USB/Storage) | Local, NAS, UPnP | Local, NAS, Streaming | Local, NAS, NFS/SMB | Local, NAS | Local, NAS, UPnP |
| | **Metadata Quality** | Basic (ID3 tags) | Good (Premium) | **Excellent** | Basic (ID3 tags) | Good (MusicBrainz/Art) | Good (MusicBrainz/Art) |
| | **Discovery** | Basic | AI Supersearch | "Valence" Recs | Basic | Good (Smart Playlists) | Good (Smart Playlists, Mix) |
| **Advanced** | **Multi-room** | **No** | Yes (Premium) | **Yes** (Zone grouping) | Via Snapcast/Pulse | No (Client-side) | **Yes** (Native sync) |
| | **DSP / EQ** | **Yes** (Parametric EQ, Filters) | Yes (Plugins) | **Yes** (MUSE) | Basic (via Sox/Plugins) | No | Basic (via plugins) |
| | **Mobile App** | **Web UI** (PWA) | iOS / Android App | iOS / Android / ARC | Vast ecosystem (3rd party) | Subsonic-compatible | iPeng, Material Skin (Web) |
| **Hardware / DIY** | **Integration** | **High** (OLED, VU, IR, HomeAssistant, USB) | High (Plugins) | Low (Software focused) | **Extreme** (Foundation for DIY) | Low (API-driven) | High (Squeezebox HW ecosystem) |
| | **CD Playback/Rip** | No | Yes (Premium) | Yes (CD Ripper) | Yes | No | Yes (via plugins) |

## Summary

*   **rsplayer**: A lightweight, efficient, and free solution ideal for **DIY enthusiasts**. Setup is as simple as running a single install script — no OS image, no separate client required. It excels in direct hardware integration (OLEDs, VU meters, IR) and offers low-level control via **HA and USB command channels**. It now features a robust **Rust-based DSP engine** with parametric EQ and various audio filters.
*   **Volumio**: A freemium platform that bridges the gap between DIY and commercial solutions. Its premium tier provides modern streaming integrations (Spotify/Tidal Connect) and multi-room audio. Setup is easy via a dedicated OS image.
*   **Roon**: The premium audiophile standard for music management. It offers the most sophisticated metadata, DSP features, and a robust multi-room ecosystem (RAAT), though at a higher cost and hardware requirement. Setup involves installing a Core, configuring endpoints, and purchasing a subscription.
*   **MPD (Music Player Daemon)**: The industrial-strength foundation for DIY audio. Its simple, stable protocol and decoupled architecture make it the go-to for custom-built players. It is extremely hackable, allowing users to pipe audio, script interactions, and choose from a massive library of community-built clients and integrations. Setup complexity is the highest of the group — MPD itself is just a daemon; you must separately install and configure a client, and there is no built-in web UI.
*   **Navidrome**: A modern, self-hosted music server focused on the "Personal Cloud" experience. By implementing the Subsonic API, it provides instant compatibility with dozens of high-quality mobile apps. It is best for users who want to stream their own collection to multiple devices with a polished, Spotify-like interface. Setup is straightforward via Docker or a single binary.
*   **LMS (Logitech Media Server)**: A mature, feature-rich server originally built for Squeezebox hardware but now running on virtually any device via software players (Squeezelite). Native multi-room sync, a large plugin ecosystem (Spotify, Deezer, smart playlists), and good metadata make it a strong all-rounder. Setup requires running a Perl-based server and at least one client (hardware or software), which adds moderate complexity compared to all-in-one solutions.
