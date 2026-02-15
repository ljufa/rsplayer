# Feature Parity Comparison: rsplayer vs. Volumio vs. Roon

This document provides a feature comparison between **rsplayer**, **Volumio**, and **Roon** as of January 2026.

| Feature Category | Feature | **rsplayer** | **Volumio** | **Roon** |
| :--- | :--- | :--- | :--- | :--- |
| **Core** | **Architecture** | Headless Service + Web UI | OS / Headless Service + Web UI + App | Core + Remote + Endpoint (RAAT) |
| | **Target Hardware** | SBCs (RPi), x86 Linux | SBCs (RPi), x86, Official Streamers | PC/Mac/Linux (Core), Roon Ready Devices |
| | **License / Cost** | Open Source (MIT) / **Free** | Freemium (Free Core / Sub for Premium) | Proprietary / **Subscription** ($$$) |
| **Playback** | **Audio Engine** | Rust (Symphonia + Cpal) | MPD (Customized) | Roon Advanced Audio Transport (RAAT) |
| | **Codecs** | **Comprehensive** (FLAC, MP3, WAV, AAC, ALAC, OGG, DSD, etc.) | Comprehensive + DSD | Comprehensive + MQA + DSD |
| | **Audio Output** | ALSA (Bit-perfect capable), DSD Native/DoP | ALSA, I2S, USB | RAAT, AirPlay, Chromecast, Sonos, USB |
| | **Streaming Services**| **None** (Planned: Spotify, Tidal) | Spotify, Tidal (Connect), Qobuz, HighResAudio | Tidal, Qobuz, KKBOX |
| | **Internet Radio** | **Yes** (Metadata supported) | Yes (vTuner, etc.) | Yes (Live Radio directory) |
| **Library & Data** | **Library Source** | Local Files (USB/Storage) | Local, NAS, UPnP/DLNA | Local, NAS, Streaming Services (Unified) |
| | **Metadata Quality** | Basic (ID3 tags from files) | Good (Rich metadata in Premium) | **Excellent** (Rich Bios, Credits, Reviews) |
| | **Discovery** | Basic (Browse/Search) | AI Supersearch (Premium) | "Valence" Recommendations, Daily Mixes |
| **Advanced** | **Multi-room** | **No** | Yes (Premium - Sync Playback) | **Yes** (High-res, Zone grouping) |
| | **DSP / EQ** | **No** (Planned) | Yes (Plugins / Fusion DSP) | **Yes** (MUSE - PEQ, Room Correction, etc.) |
| | **Mobile App** | **Web UI** (PWA-like, Responsive) | iOS / Android App | iOS / Android (Remote + ARC for on-the-go) |
| **Hardware / DIY** | **Integration** | **High** (Custom DIY: IR, OLED, VU Meters, GPIO) | High (Plugins for screens, controls) | Low (Software focused, relies on certified HW) |
| | **CD Playback/Rip** | No | Yes (Premium) | Yes (CD Ripper support) |

## Summary

*   **rsplayer**: A lightweight, efficient, and free solution ideal for **DIY enthusiasts** who want a fast, Rust-based player for local music and radio. It excels in direct hardware integration (OLEDs, VU meters, IR) but currently lacks a streaming service ecosystem.
*   **Volumio**: A freemium platform that bridges the gap between DIY and commercial solutions. Its premium tier provides modern streaming integrations (Spotify/Tidal Connect) and multi-room audio.
*   **Roon**: The premium audiophile standard for music management. It offers the most sophisticated metadata, DSP features, and a robust multi-room ecosystem (RAAT), though at a higher cost and hardware requirement.
