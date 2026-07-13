<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/_assets/banner-dark.svg">
    <img src="docs/_assets/banner-light.svg" alt="RSPlayer" width="416">
  </picture>
</p>

![](https://github.com/ljufa/rsplayer/actions/workflows/ci.yml/badge.svg)
![](https://github.com/ljufa/rsplayer/actions/workflows/cd.yml/badge.svg)
![](https://github.com/ljufa/rsplayer/actions/workflows/docker.yml/badge.svg)
![](https://img.shields.io/github/v/release/ljufa/rsplayer)
![](https://img.shields.io/github/license/ljufa/rsplayer?style=flat-square)
![](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=flat-square)

# RSPlayer

RSPlayer is an open-source, headless music server primarily for Linux, with experimental macOS and Windows builds — run it on your NAS, home server, Raspberry Pi, or any x86_64/ARM machine and control it from any browser. A native desktop app (Linux x86_64, macOS, Windows) is also available.

It runs as a systemd service and exposes a responsive web UI, making it a great fit for machines without a monitor or keyboard — but equally at home on a dedicated desktop audio PC. Under the hood it uses [Symphonia](https://github.com/pdeljanov/Symphonia) for decoding and [Cpal](https://github.com/rustaudio/cpal) for output, with a Rust-native pipeline for low-latency, high-performance playback.


📖 **Full documentation:** https://ljufa.github.io/rsplayer/

## See it in action

🖥️ **Online demo:** https://rsplayer.dlj.freemyip.com/

🎬 **One-minute video tour** — navigation, queue, library, and settings:

https://github.com/user-attachments/assets/88ba2a8e-a016-49e9-81f0-12ce53ce4ecb

## Highlights

- Pure-Rust playback engine (Symphonia + cpal) — Linux, macOS, and Windows; also a native desktop app
- Native multiroom (beta): synchronized playback across devices with automatic discovery and encrypted QUIC streaming
- Low-latency ALSA / PipeWire output, plus local playback straight to your browser
- Formats: FLAC, MP3, AAC, OGG Vorbis, WAV, AIFF, CAF, DSD (DSF/DFF), APE
- Built-in DSP: parametric EQ, filters, and presets
- EBU R128 per-song loudness normalization
- Automatic high-quality resampling when the DAC can't match the source rate
- Real-time music visualizer (12 styles) and synchronized lyrics (LRCLIB)
- Library browsing, dynamic playlists, priority queue, drag-and-drop reordering
- Network storage (SMB/CIFS, NFS) mount management from the settings page
- Home Assistant integration and optional DIY hardware control

See the [full feature list](https://ljufa.github.io/rsplayer/#/?id=features) and [feature comparison](https://ljufa.github.io/rsplayer/#/feature_parity) for details.

## Quick Start

## Linux

### Headless server
Requires `curl`. The script auto-detects your distribution (Debian/Ubuntu, Fedora/RHEL, Arch/Manjaro) and architecture (x86_64, ARM64/ARMv7/ARMv6, RISC-V 64), then installs and starts the systemd service.
```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
```

### Desktop app (flatpak or snap)
<p>
  <a href="https://snapcraft.io/rsplayer"><img height="56" alt="Get it from the Snap Store" src="https://snapcraft.io/en/dark/install.svg"></a>
</p>

Flatpak, from the [RSPlayer flatpak repo](https://ljufa.github.io/rsplayer-flatpak) (updates arrive via `flatpak update`):

```bash
flatpak install https://ljufa.github.io/rsplayer-flatpak/io.github.ljufa.rsplayer.flatpakref
```

Playback works out of the box. To enable bit-perfect direct ALSA output to USB DACs and music on host mounts like `/mnt`, see [desktop app sandbox permissions](https://ljufa.github.io/rsplayer/#/installation?id=desktop-app-flatpak-and-snap).

or deb/rmp install using script

```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install_desktop.sh)
```

Prefer to install a package manually (`.deb` / `.rpm` / `.tgz`) or run the raw binary? See the [Linux installation guide](https://ljufa.github.io/rsplayer/#/installation?id=linux).

### macOS (experimental)

No install script — download directly from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):

- **Server binary**: `rsplayer_darwin_arm64` (Apple Silicon) or `rsplayer_darwin_amd64` (Intel), then `chmod +x rsplayer && ./rsplayer`
- **Desktop app**: open the `.dmg` and drag RSPlayer to Applications

Audio output uses CoreAudio via `cpal`. See the [macOS installation guide](https://ljufa.github.io/rsplayer/#/installation?id=macos-experimental).

### Windows (experimental)

Download directly from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):

- **Server**: `rsplayer_windows_amd64.exe` — run it directly, then open `http://localhost:8000`
- **Desktop app**: run the `rsplayer-desktop_windows_amd64.exe` NSIS installer (fetches WebView2 automatically if needed)

Audio output uses WASAPI by default via `cpal`, and installed **ASIO** drivers are selectable in Settings → Audio interface for exclusive, low-latency, bit-perfect playback. See the [Windows installation guide](https://ljufa.github.io/rsplayer/#/installation?id=windows-experimental).

> ASIO is a trademark and software of Steinberg Media Technologies GmbH.

### Docker

```bash
docker run -p 8000:80 -v ${MUSIC_DIR}:/music -v rsplayer_data:/opt/rsplayer --device /dev/snd -it --rm ljufa/rsplayer:latest
```

Or use [docker compose](docker/docker-compose.yaml):

```yaml
services:
  rsplayer:
    image: ljufa/rsplayer:latest
    devices:
      - /dev/snd
    ports:
      - 8000:80
    volumes:
      - ${MUSIC_DIR}:/music:ro
      - 'rsplayer_volume:/opt/rsplayer'
    restart: unless-stopped
volumes:
  rsplayer_volume:
    driver: local
```

### Open the UI

Navigate to `http://localhost` (or the IP of the machine running RSPlayer). For the Docker command above, the UI is at `http://localhost:8000`. For configuration, see the [documentation](https://ljufa.github.io/rsplayer/#/configuration).

## Documentation

| Topic | Link |
|---|---|
| Overview & full feature list | https://ljufa.github.io/rsplayer/ |
| Installation | https://ljufa.github.io/rsplayer/#/installation |
| Configuration | https://ljufa.github.io/rsplayer/#/configuration |
| Usage guide | https://ljufa.github.io/rsplayer/#/usage |
| Troubleshooting | https://ljufa.github.io/rsplayer/#/troubleshooting |
| Building from source | https://ljufa.github.io/rsplayer/#/build |
| Feature comparison | https://ljufa.github.io/rsplayer/#/feature_parity |

## Home Assistant & DIY Hardware

RSPlayer can be controlled from [Home Assistant](https://www.home-assistant.io/) via the [rsplayer_hacs_plugin](https://github.com/ljufa/rsplayer_hacs_plugin). For DIY builds, see [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware) and [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware).

## Contributing

Contributions are welcome — submit a pull request or open an issue. To build from source, see the [Building from Source](https://ljufa.github.io/rsplayer/#/build) documentation.

## License

RSPlayer is licensed under the MIT license. See the [LICENSE](LICENSE) file for more information.
