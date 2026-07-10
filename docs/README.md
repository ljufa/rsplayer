# RSPlayer — Rust-native music server

RSPlayer is an open-source, headless music server primarily for Linux, with experimental macOS and Windows builds. Run it on your NAS, home server, Raspberry Pi, or any x86_64/ARM machine and control it from any browser. A native desktop app (Linux x86_64, macOS, Windows) is also available.

It runs as a systemd service and exposes a responsive web UI, making it a great fit for machines without a monitor or keyboard — but equally at home on a dedicated desktop audio PC. Hardware and DIY integrations (GPIO DAC control, custom firmware) are fully optional.

Under the hood RSPlayer uses [Symphonia](https://github.com/pdeljanov/Symphonia) for audio decoding and [Cpal](https://github.com/rustaudio/cpal) for output, with a Rust-native audio pipeline for low-latency, high-performance playback.

**Online demo → https://rsplayer.dlj.freemyip.com/**

## Getting Started

- [Installation](installation.md) — supported platforms, install methods, and verification
- [Configuration](configuration.md) — audio settings, volume control, and hardware integration
- [Usage Guide](usage.md) — web interface, queue, library, keyboard shortcuts
- [Troubleshooting](troubleshooting.md) — common issues and fixes

## Features

- **Low Latency Output**: Direct output to ALSA or PipeWire minimizes latency.
- **Multiroom Playback**: Native synchronized playback across multiple RSPlayer devices — automatic discovery on the LAN, encrypted QUIC streaming, millisecond-level sync, per-room volume and EQ. See [Multiroom Playback](multiroom.md).
- **Local Browser Playback**: Stream audio directly to your web browser for local playback.
- **Automatic Resampling**: High-quality FFT-based resampling (via rubato) when the output device doesn't support the source sample rate — no configuration needed. Automatically probes fallback rates for ALSA drivers that misreport their supported range.
- **Fixed Output Sample Rate**: Optionally lock the output to a specific sample rate, overriding automatic detection.
- **Multiple Music Sources**: Configure multiple local directories and network mounts as music sources, all scanned together.
- **Network Storage Management**: Discover, mount, and manage SMB/CIFS and NFS shares directly from the settings page.
- **Adjustable Playback Thread Priority**: Customize the priority of the playback thread up to a real-time rating of 99 via the settings page.
- **Dedicated CPU Core for Playback**: By default, the playback thread is pinned to a single CPU core for optimized performance.
- **Web UI Remote Control**: Manage your playback remotely with an intuitive web interface.
- **Flexible Volume Control**: Choose ALSA mixer, PipeWire, or software gain volume control, with hardware control also supported via RSPlayer firmware integration.
- **Volume Persistence**: Volume level is saved on change and restored on restart, defaulting to 0 on first use to prevent hardware-max shock.
- **Comprehensive Music Library Management**: Scan, search, and browse your music library and online radio stations with ease.
- **Dynamic Playlists**: Automatically create dynamic playlists for personalized listening experiences.
- **Playlists by Genre, Year**: Browse and create playlists based on genre or year.
- **Drag-and-Drop Queue Reordering**: Reorder queue items by dragging them directly in the queue view.
- **DSP Integration**: Advanced Digital Signal Processing with parametric EQ, filters, and presets.
- **Loudness Normalization**: Per-song EBU R128 loudness normalization, toggleable from the settings page. Analysis runs automatically in the background while playback is stopped and results are stored permanently.
- **Music Visualizer**: Real-time audio visualization in the web interface, with 12 animated visualizer styles.
- **Synchronized Lyrics**: Real-time synchronized lyrics support via LRCLIB integration.
- **Library Statistics**: Dedicated statistics page showing song/album/artist counts, total duration, play history, top genres, albums by decade, and loudness analysis progress.
- **Web UI Themes**: Support for customizable themes and dark/light modes (10+ built-in themes).
- **Global Keyboard Shortcuts**: Full keyboard control for playback, navigation, and search (Space, arrows, M, L, Y, S, /, ?, 1-4, F/A/P/R/T).
- **Breadcrumb Navigation**: Clear navigation context on all library pages.
- **Skeleton Loading & Empty States**: Visual feedback during loading and helpful empty state screens.
- **Written in Rust**: Enjoy the benefits of minimal dependencies and high performance, thanks to the Rust-native implementation.
- **Extended Hardware Control**: Support for seek and power management via firmware interactions.

### Supported Formats

FLAC, MP3, AAC, OGG Vorbis, WAV, AIFF, CAF, DSD (DSF/DFF), APE (Monkey's Audio), and SACD ISO disc images.

## Known Limitations

- **DSD passthrough bypass**: DSP (parametric EQ, filters), loudness normalization, and resampling are all bypassed for DSD files (`.dsf`, `.dff`). DSD bitstreams are passed directly to the DAC without any signal processing.
- **Radio streams**: Loudness normalization is not applied to internet radio streams — it requires pre-scanned file metadata. Seeking is not supported for streams.
- **Unsupported formats**: Opus, WMA, WavPack, and TTA are not supported.
- **Local Browser Playback**: In this mode the browser's native audio engine plays files directly. DSP, loudness normalization, resampling, visualization, and DSD playback are all unavailable — format support is limited to what the browser itself can decode.
- **Non-Linux builds**: On macOS and Windows, network share mounting, ALSA/PipeWire volume, IR remote, system poweroff/reboot, and firmware USB integration are unavailable.

## How does RSPlayer compare?

> Approximate comparison as of early 2026. Features change frequently. See [Feature Parity](feature_parity.md) for a detailed comparison against Volumio, Roon, MPD, Navidrome, LMS, moOde, Daphile, and piCorePlayer.

| Feature | RSPlayer | Volumio | Moode Audio | MPD |
|---|---|---|---|---|
| Language | Rust | Node.js | PHP/Bash | C |
| Playback engine | Symphonia + cpal (pure rust) | MPD (plugin-based) | MPD (plugin-based) | plugin-based (FFmpeg, libFLAC, …) |
| OS support | Linux, macOS, Windows | Linux | Linux (Pi) | Linux, macOS, Windows |
| Native desktop app variant | Linux, macOS, Windows | — | — | — |
| Web UI | ✓ | ✓ | ✓ | 3rd party |
| Local browser playback | ✓ | — | — | — |
| Parametric EQ / DSP | ✓ built-in | paid tier | ✓ (CamillaDSP) | via plugins |
| Multi-room | planned | paid tier | ✓ | via plugins |
| DSD playback | ✓ | ✓ | ✓ | ✓ |
| Loudness normalization (EBU R128) | ✓ | — | — | — |
| Synchronized lyrics | ✓ | — | — | — |
| Docker | ✓ | ✓ | — | ✓ |
| RISC-V 64 | ✓ | — | — | — |
| Home Assistant integration | ✓ | ✓ | — | ✓ |
| DIY hardware integration | optional | — | — | — |
| License | MIT | GPL | GPL | GPL |

## Home Assistant Integration

RSPlayer can be controlled from [Home Assistant](https://www.home-assistant.io/) via the [rsplayer_hacs_plugin](https://github.com/ljufa/rsplayer_hacs_plugin).

Features include media player control (play, pause, stop, next/prev, volume) and real-time sync with `rsplayer_firmware` power state.

Install via HACS by adding `https://github.com/ljufa/rsplayer_hacs_plugin` as a custom repository.

## DIY Hardware

For DIY enthusiasts, RSPlayer offers the flexibility to integrate with custom hardware components.

- **Hardware Designs**: [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware)
- **Firmware**: [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware)

See the [Configuration](configuration.md) page for hardware integration details.

## Planned Features

- **Expanded Audio Codec Support**: Compatibility with a wider range of audio codecs.
- **Intelligent Dynamic Playlists**: Advanced dynamic playlists that adapt based on user likes or playback counts.
- **Music Recommendations**: Suggest similar tracks or artists based on listening history or current playback.
- **Generate Missing Album Cover Image**: Auto-generate album art using the album name.
- **MPD protocol support**: Compatibility with MPD clients.
- **Subsonic protocol support**: Compatibility with Subsonic clients.
- **Community plugin framework**: Extensible architecture for third-party plugins.
- **Improved playlist management**: Create/modify/delete items and playlists from everywhere.
- **In-app upgrade**: Detect a new version and provide an upgrade button.
- **Problem (bug) report with diagnostics**: Report problems directly from the app.
- **Scheduled music library scans**: Define an automatic library scan interval (or cron).
- **Desktop App File Logging**: Rolling log files for the desktop app, so users can capture and share diagnostics without running from a terminal.
- **Homebrew Distribution**: Install and update RSPlayer on macOS through a Homebrew formula/cask.
- **Android App**: A native Android build (with an optional kiosk mode for dedicated players), published to the Google Play Store.
- **Automatic Linux Updates**: A hosted apt/dnf package repository — or an in-app self-update — so Linux installs receive new versions without re-running the install script.
- **AUR Package**: An official Arch User Repository package for Arch Linux and derivatives.


## Contributing

Contributions are welcome — submit a pull request or open an issue on the [GitHub repository](https://github.com/ljufa/rsplayer). To build from source, see [Building from Source](build.md).

## License

RSPlayer is licensed under the MIT license. See the [LICENSE](https://github.com/ljufa/rsplayer/blob/master/LICENSE) file for more information.
