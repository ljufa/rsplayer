# Installation

RSPlayer comes in two variants:

- **Server** — runs in the background (as a systemd service on Linux) and you control it from a browser or phone. Best for a Raspberry Pi, NAS or always-on audio PC. Available for every supported architecture.
- **Desktop app** — a normal app window on the computer you're using. Available for Linux (x86_64 and ARM64), macOS, Windows and Android.

**Which one do I need?**

| | Linux | macOS | Windows | Android |
|---|---|---|---|---|
| **Server** | [Install script](?id=linux-server) | [Download](?id=macos) | [Download](?id=windows) | — |
| **Desktop app** | [Snap / Flatpak](?id=desktop-app-flatpak-and-snap) or [native package](?id=desktop-app-native-package) | [Download](?id=macos) | [Download](?id=windows) | [APK](?id=android) |
| **Docker** | [docker run](?id=docker) | — | — | — |

## Linux server

Works on Debian, Ubuntu, Raspberry Pi OS, Fedora, openSUSE and Arch, on x86_64, ARM64, ARMv7, ARMv6 (every Raspberry Pi) and RISC-V 64:

```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/main/install.sh)
```

The script detects your distribution and architecture, installs RSPlayer and starts the service. At the end it prints the address to open in your browser — usually `http://<device-ip>`, for example `http://raspberrypi.local`.

On Debian/Ubuntu and Fedora/openSUSE it adds the [RSPlayer package repository](https://ljufa.github.io/rsplayer-pkg), so future updates arrive with your regular `apt upgrade` / `dnf upgrade`. On Arch it installs the release tarball. Run the same command again to upgrade on Arch, or add `--pre-release` to try the latest pre-release.

If the page doesn't load:

```bash
sudo systemctl status rsplayer     # should say "active (running)"
journalctl -u rsplayer -f -n 50    # recent logs
```

?> The HTTP/HTTPS ports and bind address are set in `/opt/rsplayer/env`: `PORT=80`, `TLS_PORT=443` and `BIND_ADDR=0.0.0.0` (all interfaces) by default. Restart the service after changing them. See [Troubleshooting](troubleshooting.md) for common problems.

### Add the package repository manually

This is what the install script does on deb and rpm distributions. The repositories are GPG-signed.

Debian / Ubuntu / Raspberry Pi OS (amd64, arm64, armhf, riscv64):

```bash
sudo curl -fsSL -o /usr/share/keyrings/rsplayer.gpg https://ljufa.github.io/rsplayer-pkg/rsplayer.gpg
echo "deb [signed-by=/usr/share/keyrings/rsplayer.gpg] https://ljufa.github.io/rsplayer-pkg/deb stable main" | sudo tee /etc/apt/sources.list.d/rsplayer.list
sudo apt update && sudo apt install rsplayer
```

Fedora / RHEL / openSUSE (x86_64, aarch64, armv6hl, armv7hl, riscv64):

```bash
sudo curl -fsSL -o /etc/yum.repos.d/rsplayer.repo https://ljufa.github.io/rsplayer-pkg/rpm/rsplayer.repo
sudo dnf install rsplayer
```

!> The apt repository's `armhf` package is the ARMv6 build, which runs on every 32-bit Raspberry Pi including the Zero/1. If you want the ARMv7-optimized build on a 32-bit OS, install the `rsplayer_*_armhfv7.deb` release file manually.

### Install a package file manually

Download the file for your system from the [latest release](https://github.com/ljufa/rsplayer/releases/latest) — see [release file names](?id=release-file-names) — and install it:

```bash
sudo apt install ./rsplayer_*_arm64.deb     # Debian / Ubuntu / Raspberry Pi OS
sudo dnf install ./rsplayer_*_x86_64.rpm    # Fedora / RHEL / openSUSE
```

On Arch, the `.tgz` is extracted to `/` — the install script handles the required users and groups for you.

### Run the binary without installing

1. Download the `rsplayer_*` binary for your architecture from the [latest release](https://github.com/ljufa/rsplayer/releases/latest) and rename it to `rsplayer`.
2. Run it with `chmod +x rsplayer && ./rsplayer`, then open `http://localhost:8000`.
3. To run it as a service, use [this systemd unit](https://github.com/ljufa/rsplayer/blob/main/PKGS/debian/etc/systemd/system/rsplayer.service).

## Desktop app

### Desktop app (Flatpak and Snap)

<p>
  <a href="https://snapcraft.io/rsplayer"><img class="store-badge" height="56" alt="Get it from the Snap Store" src="https://snapcraft.io/en/dark/install.svg"></a>
</p>

**Snap** (x86_64, ARM64):

```bash
sudo snap install rsplayer
# Direct ALSA (hw:) access for bit-perfect output to USB DACs (not connected automatically)
sudo snap connect rsplayer:alsa
# Music on removable drives and host mounts (/media, /run/media, /mnt)
sudo snap connect rsplayer:removable-media
```

Without `rsplayer:alsa` connected, playback still works through the virtual "Pipewire" output; connecting it makes hw: cards appear in Settings → Playback for bit-perfect output.

**Flatpak** (x86_64, ARM64), from the [RSPlayer flatpak repo](https://ljufa.github.io/rsplayer-flatpak):

```bash
flatpak install https://ljufa.github.io/rsplayer-flatpak/io.github.ljufa.rsplayer.flatpakref
```

Updates arrive with `flatpak update`. A single-file `.flatpak` bundle is also attached to every [release](https://github.com/ljufa/rsplayer/releases/latest) for offline installs.

The Flatpak plays through PipeWire, has direct (bit-perfect) ALSA access to USB DACs, and can read music from `~/Music`, removable drives and host mounts (`/media`, `/run/media`, `/mnt`). If these permissions were revoked (for example with Flatseal), re-enable them:

```bash
# Music on host mounts, e.g. /mnt (read-only)
flatpak override --user io.github.ljufa.rsplayer --filesystem=/mnt:ro
# Direct ALSA (hw:) access for bit-perfect output to USB DACs
flatpak override --user io.github.ljufa.rsplayer --device=all
```

Grant other music folders with [Flatseal](https://flathub.org/apps/com.github.tchx84.Flatseal) or `flatpak override --user io.github.ljufa.rsplayer --filesystem=...`. Symlinks only resolve if the target path is also granted.

?> Network-share mounting and system power actions are unavailable inside the Snap and Flatpak sandboxes. Use the native package below, or the server, if you need them.

### Desktop app (native package)

Installs the `.deb` / `.rpm` from the package repository (x86_64 and ARM64), or the release tarball on Arch:

```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/main/install_desktop.sh)
```

If you already added the [package repository](?id=add-the-package-repository-manually), you can also run `sudo apt install rsplayer-desktop` or `sudo dnf install rsplayer-desktop`. Launch RSPlayer from your application menu.

## macOS

Download from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):

- **Desktop app:** open the `.dmg` and drag RSPlayer to Applications.
- **Server:** download `rsplayer_darwin_arm64` (Apple Silicon) or `rsplayer_darwin_amd64` (Intel), rename it to `rsplayer`, then:

  ```bash
  chmod +x rsplayer
  ./rsplayer
  ```

  Open `http://localhost:8000`.

?> Network mount management, Linux power actions and firmware USB integration are unavailable on macOS.

## Windows

Download from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):

- **Desktop app:** run the `rsplayer-desktop_windows_amd64.exe` installer. It downloads [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) automatically if it's missing (it's included with Windows 10/11).
- **Server:** run `rsplayer_windows_amd64.exe` — no installation needed — then open `http://localhost:8000`.

Audio uses WASAPI by default. Installed **ASIO** drivers can be selected in Settings → Audio interface (shown as `… (ASIO)`) for exclusive, low-latency, bit-perfect playback — set the sample rate and buffer size in the driver's own control panel.

> ASIO is a trademark and software of Steinberg Media Technologies GmbH.

?> Network mount management, Linux power actions, ALSA/PipeWire volume, IR remote and firmware USB integration are unavailable on Windows.

## Android

The Android app is the desktop app on a phone or tablet: the RSPlayer server runs inside the app and plays through the device's audio output (AAudio), so it works offline with music stored on the device. Download `rsplayer_<version>_android.apk` from the [latest release](https://github.com/ljufa/rsplayer/releases/latest) and open it on the device (allow installs from unknown sources when asked). Android 8.0 or newer on an ARM phone or tablet (64-bit ARM, or 32-bit ARMv7 on older devices). There is no x86_64 build; to run it on an emulator or a Chromebook, build it from source (see the build guide).

On first start the app asks for **music and notification** permissions. Put your music in the shared **Music** folder (`/storage/emulated/0/Music`, what a computer shows as `Music` over USB) — it is scanned automatically once the permission is granted; more folders can be added in Settings → Music library. Playback keeps going with the screen off and is controllable from the lock screen, the notification and headset buttons.

?> Android only lets the app see audio and image files in shared storage: `.cue`, `.m3u` and `.lrc` sidecar files next to the music are invisible, and SMB/NFS network mounts are unavailable. Podcasts and internet radio work as on the desktop. Multiroom peer discovery works on Wi-Fi. A Google Play listing is planned.

## Docker

```bash
docker run -p 8000:80 -v ${MUSIC_DIR}:/music -v rsplayer_data:/opt/rsplayer --device /dev/snd -it --rm ljufa/rsplayer:latest
```

Then open `http://localhost:8000`. A ready-made [docker-compose.yaml](https://github.com/ljufa/rsplayer/blob/main/docker/docker-compose.yaml) is in the repository.

## Reference

### Supported platforms

| Architecture | Typical devices | Debian / Ubuntu / Raspberry Pi OS | Fedora / RHEL / openSUSE | Arch / Manjaro | Docker | Nix |
|:---|:---|:---|:---|:---|:---:|:---:|
| **x86_64** | Intel/AMD PCs, servers, NAS | `.deb` **S+D** | `.rpm` **S+D** | `.tgz` **S+D** | ✓ | ✓ |
| **ARM64** (aarch64) | RPi 4, RPi 5, ARMv8 boards | `.deb` **S+D** | `.rpm` **S+D** | `.tgz` **S+D** | — | ✓ |
| **ARMv7** | RPi 2, RPi 3, 32-bit RPi 4 | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |
| **ARMv6** | RPi Zero, RPi Zero W, RPi 1 | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |
| **RISC-V 64** | RISC-V 64-bit boards | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |

**S** = server, **D** = desktop app. macOS (Apple Silicon and Intel) and Windows (x86_64) have both a server binary and a desktop app. Android (ARM64 and ARMv7, Android 8.0+) has the desktop app as an APK.

Not supported yet: FreeBSD.

### Release file names

| Architecture | `.deb` | `.rpm` | `.tgz` (Arch) |
|:---|:---|:---|:---|
| x86_64 | `amd64` | `x86_64` | `amd64` |
| aarch64 | `arm64` | `aarch64` | `arm64` |
| armv7 | `armhfv7` | `armv7hl` | `armhfv7` |
| armv6 | `armhfv6` | `armv6hl` | `armhfv6` |
| riscv64 | `riscv64` | `riscv64` | `riscv64` |

Examples: `rsplayer_<version>_arm64.deb` (server), `rsplayer-desktop_<version>_amd64.deb` (desktop app), `rsplayer-desktop_<version>_amd64.tgz` (desktop app for Arch), `rsplayer_darwin_arm64` (macOS server binary), `rsplayer_<version>_android.apk` (Android app, all ABIs in one file).
