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

RSPlayer is an open-source music player written in Rust. Run it as a **headless server** on a NAS, home server or Raspberry Pi and control it from any browser or phone — or install the **desktop app** on your computer.

🎧 **[Online demo](https://demo.rsplayer.de)** · 📖 **[Documentation](https://docs.rsplayer.de/)** · ⬇️ **[Latest release](https://github.com/ljufa/rsplayer/releases/latest)**

https://github.com/user-attachments/assets/7cf6ef93-2251-4f85-a1cb-be5865a257d5

## Highlights

- Pure-Rust playback engine ([Symphonia](https://github.com/pdeljanov/Symphonia) + [cpal](https://github.com/rustaudio/cpal)) with low-latency ALSA / PipeWire output, or play straight in your browser
- FLAC, MP3, AAC, OGG Vorbis, WAV, AIFF, CAF, DSD (DSF/DFF), APE
- Multiroom: synchronized playback across devices with automatic discovery
- Parametric EQ and DSP presets, EBU R128 loudness normalization, automatic resampling
- Internet radio and a podcast client — subscribe to shows and resume episodes where you stopped
- Visualizer, synchronized lyrics, library browsing, dynamic playlists
- SMB/NFS network share mounting, Home Assistant integration, DIY hardware control

See the [full feature list](https://docs.rsplayer.de/#/?id=features) and [feature comparison](https://docs.rsplayer.de/#/feature_parity).

## Install

**Which one do I need?**

| | Linux | macOS · Windows | Android |
|---|---|---|---|
| **Server** — runs in the background, you control it from a browser or phone. Best for a Raspberry Pi, NAS or always-on audio PC. | [Install script](#linux-server) | [Download](#macos-and-windows) | — |
| **Desktop app** — a normal app window on the computer you're using. | [Snap / Flatpak / script](#linux-desktop-app) | [Download](#macos-and-windows) | [APK](#android) |
| **Docker** | [docker run](#docker) | — | — |

### Linux server

Works on Debian, Ubuntu, Raspberry Pi OS, Fedora and Arch, on x86_64, ARM (every Raspberry Pi) and RISC-V:

```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/main/install.sh)
```

Then open **`http://<device-ip>`** (for example `http://raspberrypi.local`) in a browser. Updates come with your normal `apt upgrade` / `dnf upgrade`.

<details>
<summary>Prefer to add the package repository yourself?</summary>

Debian / Ubuntu / Raspberry Pi OS:

```bash
sudo curl -fsSL -o /usr/share/keyrings/rsplayer.gpg https://ljufa.github.io/rsplayer-pkg/rsplayer.gpg
echo "deb [signed-by=/usr/share/keyrings/rsplayer.gpg] https://ljufa.github.io/rsplayer-pkg/deb stable main" | sudo tee /etc/apt/sources.list.d/rsplayer.list
sudo apt update && sudo apt install rsplayer
```

Fedora / RHEL / openSUSE:

```bash
sudo curl -fsSL -o /etc/yum.repos.d/rsplayer.repo https://ljufa.github.io/rsplayer-pkg/rpm/rsplayer.repo
sudo dnf install rsplayer
```

`.deb`, `.rpm` and `.tgz` files for manual install are on the [release page](https://github.com/ljufa/rsplayer/releases/latest).
</details>

### Linux desktop app

<a href="https://snapcraft.io/rsplayer"><img height="48" alt="Get it from the Snap Store" src="https://snapcraft.io/en/dark/install.svg"></a>

```bash
sudo snap install rsplayer
# or
flatpak install https://ljufa.github.io/rsplayer-flatpak/io.github.ljufa.rsplayer.flatpakref
# or a native .deb/.rpm package
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/main/install_desktop.sh)
```

The Snap and Flatpak are sandboxed. For bit-perfect output to USB DACs, see [sandbox permissions](https://docs.rsplayer.de/#/installation?id=desktop-app-flatpak-and-snap).

### macOS and Windows

Download from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):

| | Server | Desktop app |
|---|---|---|
| **macOS** | `rsplayer_darwin_arm64` (Apple Silicon) or `rsplayer_darwin_amd64` (Intel) — `chmod +x` and run | `.dmg` |
| **Windows** | `rsplayer_windows_amd64.exe` — just run it | `rsplayer-desktop_windows_amd64.exe` |

After starting the server, open `http://localhost:8000`. On Windows, installed ASIO drivers can be selected in Settings → Audio interface. More in the [macOS](https://docs.rsplayer.de/#/installation?id=macos) and [Windows](https://docs.rsplayer.de/#/installation?id=windows) guides.

### Android

Download `rsplayer_<version>_android.apk` from the [latest release](https://github.com/ljufa/rsplayer/releases/latest) and open it on the phone (Android 8.0+). The full player runs inside the app: put music in the shared **Music** folder, grant the media permission on first start, and playback works offline with lock-screen controls. Details in the [Android guide](https://docs.rsplayer.de/#/installation?id=android).

> ASIO is a trademark and software of Steinberg Media Technologies GmbH.

### Docker

```bash
docker run -p 8000:80 -v ${MUSIC_DIR}:/music -v rsplayer_data:/opt/rsplayer --device /dev/snd -it --rm ljufa/rsplayer:latest
```

Then open `http://localhost:8000`.

<details>
<summary>docker compose</summary>

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
</details>

**Next steps:** [configuration](https://docs.rsplayer.de/#/configuration) · [usage guide](https://docs.rsplayer.de/#/usage) · [troubleshooting](https://docs.rsplayer.de/#/troubleshooting)

## Home Assistant & DIY hardware

Control RSPlayer from [Home Assistant](https://www.home-assistant.io/) with the [rsplayer_hacs_plugin](https://github.com/ljufa/rsplayer_hacs_plugin). For DIY builds, see [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware) and [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware).

## Contributing

Contributions are welcome — open an issue or a pull request. See [Building from source](https://docs.rsplayer.de/#/build).

## License

MIT — see [LICENSE](LICENSE).
