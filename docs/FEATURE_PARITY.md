# Feature Parity Comparison: rsplayer vs. Others

This document provides a feature comparison between **rsplayer** and other popular music playback solutions as of January 2026.

| Feature Category | Feature | **rsplayer** | **Volumio** | **Roon** | **MPD** | **Navidrome** |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Core** | **Architecture** | Headless Service + Web UI | OS / Headless + Web UI | Core + Remote + RAAT | Client-Server (Daemon) | Client-Server (Web/API) |
| | **Target Hardware** | SBCs (RPi), x86 Linux | SBCs, x86, Streamers | PC/Mac/Linux, Roon Ready | Linux, BSD, macOS, Win | PC, Server, NAS, SBCs |
| | **License / Cost** | Open Source / **Free** | Freemium | Subscription ($$$) | Open Source / **Free** | Open Source / **Free** |
| **Playback** | **Audio Engine** | Rust (Symphonia) | MPD (Customized) | RAAT | C++ (Native) | Go + FFmpeg |
| | **Codecs** | **Comprehensive** | Comprehensive + DSD | Comprehensive + MQA | **Very Comprehensive** | Comprehensive |
| | **Audio Output** | ALSA, DSD Native/DoP | ALSA, I2S, USB | RAAT, AirPlay, USB | ALSA, Pulse, PipeWire | Web Browser, API Stream |
| | **Streaming Services**| **None** (Planned) | Spotify, Tidal, Qobuz | Tidal, Qobuz, KKBOX | Limited (via plugins) | **None** (Self-hosted) |
| | **Internet Radio** | **Yes** | Yes | Yes | Yes | Yes |
| **Library & Data** | **Library Source** | Local Files (USB/Storage) | Local, NAS, UPnP | Local, NAS, Streaming | Local, NAS, NFS/SMB | Local, NAS |
| | **Metadata Quality** | Basic (ID3 tags) | Good (Premium) | **Excellent** | Basic (ID3 tags) | Good (MusicBrainz/Art) |
| | **Discovery** | Basic | AI Supersearch | "Valence" Recs | Basic | Good (Smart Playlists) |
| **Advanced** | **Multi-room** | **No** | Yes (Premium) | **Yes** (Zone grouping) | Via Snapcast/Pulse | No (Client-side) |
| | **DSP / EQ** | **Yes** (Parametric EQ, Filters) | Yes (Plugins) | **Yes** (MUSE) | Basic (via Sox/Plugins) | No |
| | **Mobile App** | **Web UI** (PWA) | iOS / Android App | iOS / Android / ARC | Vast ecosystem (3rd party) | Subsonic-compatible |
| **Hardware / DIY** | **Integration** | **High** (OLED, VU, IR, HomeAssistant, USB) | High (Plugins) | Low (Software focused) | **Extreme** (Foundation for DIY) | Low (API-driven) |
| | **CD Playback/Rip** | No | Yes (Premium) | Yes (CD Ripper) | Yes | No |

## Summary

*   **rsplayer**: A lightweight, efficient, and free solution ideal for **DIY enthusiasts**. It excels in direct hardware integration (OLEDs, VU meters, IR) and offers low-level control via **HA and USB command channels**. It now features a robust **Rust-based DSP engine** with parametric EQ and various audio filters.
*   **Volumio**: A freemium platform that bridges the gap between DIY and commercial solutions. Its premium tier provides modern streaming integrations (Spotify/Tidal Connect) and multi-room audio.
*   **Roon**: The premium audiophile standard for music management. It offers the most sophisticated metadata, DSP features, and a robust multi-room ecosystem (RAAT), though at a higher cost and hardware requirement.
*   **MPD (Music Player Daemon)**: The industrial-strength foundation for DIY audio. Its simple, stable protocol and decoupled architecture make it the go-to for custom-built players. It is extremely hackable, allowing users to pipe audio, script interactions, and choose from a massive library of community-built clients and integrations.
*   **Navidrome**: A modern, self-hosted music server focused on the "Personal Cloud" experience. By implementing the Subsonic API, it provides instant compatibility with dozens of high-quality mobile apps. It is best for users who want to stream their own collection to multiple devices with a polished, Spotify-like interface.
