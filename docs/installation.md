# Installation

## Supported Platforms

RSPlayer is available in two variants:

- **Server** — The headless music server daemon. Runs as a systemd service (Linux) or standalone binary (macOS), controlled from any web browser. Available for all supported architectures.
- **Desktop** — A standalone desktop application with a native window, built with Tauri. Available for **x86_64 Linux** and **macOS** only.

### Linux

| Architecture | Typical Devices | Debian / Ubuntu / Raspbian | Fedora / RHEL / openSUSE | Arch / Manjaro | Docker | Nix |
|:---|:---|:---|:---|:---|:---:|:---:|
| **x86_64** | Intel/AMD PCs, servers, NAS | `.deb` **S+D** | `.rpm` **S+D** | `.tgz` **S+D** | ✓ | ✓ |
| **ARM64** (aarch64) | RPi 4, RPi 5, ARMv8 boards | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |
| **ARMv7** | RPi 2, RPi 3, 32-bit RPi 4 | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |
| **ARMv6** | RPi Zero, RPi Zero W, RPi 1 | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |
| **RISC-V 64** | RISC-V 64-bit boards | `.deb` S | `.rpm` S | `.tgz` S | — | ✓ |

**S** = Server (headless daemon) — all architectures  
**D** = Desktop (native GUI app) — x86_64 only; packages prefixed `rsplayer-desktop_` (`.deb`, `.rpm`, `.tgz`)

Release filename suffixes by architecture:

| Architecture | `.deb` suffix | `.rpm` suffix | `.tgz` suffix |
|:---|:---|:---|:---|
| x86_64 | `amd64` | `x86_64` | `amd64` |
| aarch64 | `arm64` | `aarch64` | `arm64` |
| armv7 | `armhfv7` | `armv7hl` | `armhfv7` |
| armv6 | `armhfv6` | `armv6hl` | `armhfv6` |
| riscv64 | `riscv64` | `riscv64` | `riscv64` |

Example release asset names: `rsplayer_3.5.6_amd64.deb` (server), `rsplayer-desktop_3.5.6_amd64.deb` (desktop deb), `rsplayer-desktop_3.5.6_amd64.tgz` (desktop Arch tarball).

### macOS (experimental)

| Architecture | Typical Devices | Server | Desktop |
|:---|:---|:---|:---|
| **Apple Silicon** (`aarch64-apple-darwin`) | M1/M2/M3/M4 Macs | raw binary (`rsplayer_darwin_arm64`) | DMG |
| **Intel** (`x86_64-apple-darwin`) | Intel Macs | raw binary (`rsplayer_darwin_amd64`) | DMG |

> Network mount management, Linux power actions, and firmware USB integration are unavailable on macOS.

### Unsupported Platforms

The following platforms are not currently supported but may be considered in the future:

- Android
- Windows (x86_64 / aarch64)
- FreeBSD

## Install or upgrade
RSPlayer can be installed using one of two methods:
* Using installation script (automatically detects your distribution and architecture)
```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
```
The installation script detects your Linux distribution (Debian/Ubuntu, Fedora/RHEL/CentOS, Arch/Manjaro) and installs the appropriate package type (.deb, .rpm, or .tgz tarball).

?> macOS does not use the Linux package install script. See the [macOS section](#macos-experimental) below for server and desktop download instructions.

* Manually download and install package
The latest packages can be downloaded from [this page](https://github.com/ljufa/rsplayer/releases/latest). Available package types:
- **DEB packages**: For Debian, Ubuntu, Raspbian — `rsplayer_*_amd64.deb`, `rsplayer_*_arm64.deb`, `rsplayer_*_armhfv7.deb`, `rsplayer_*_armhfv6.deb`, `rsplayer_*_riscv64.deb`, and `rsplayer-desktop_*_amd64.deb`
- **RPM packages**: For Fedora, RHEL, CentOS, openSUSE — `rsplayer_*_x86_64.rpm`, `rsplayer_*_aarch64.rpm`, `rsplayer_*_armv7hl.rpm`, `rsplayer_*_armv6hl.rpm`, `rsplayer_*_riscv64.rpm`, and `rsplayer-desktop-*.x86_64.rpm`
- **Arch tarballs**: For Arch Linux, Manjaro — `rsplayer_*_amd64.tgz`, `rsplayer_*_arm64.tgz`, `rsplayer_*_armhfv7.tgz`, `rsplayer_*_armhfv6.tgz`, `rsplayer_*_riscv64.tgz` (server), and `rsplayer-desktop_*_amd64.tgz` (desktop)

* Download and manually install binary file
  - Under latest release page find `rsplayer_*` file for your system and download
  - rename file to `rsplayer`
  - make it executable using `chmod +x rsplayer`
  - run using command `./rsplayer`
  - optionally if you need to run rsplayer automatically as a service use [this systemd service file](../PKGS/debian/etc/systemd/system/rsplayer.service)

### macOS (experimental) quick run

**Option 1 — Server binary:**

1. Download the binary from the [latest release](https://github.com/ljufa/rsplayer/releases/latest):
   - `rsplayer_darwin_arm64` for Apple Silicon
   - `rsplayer_darwin_amd64` for Intel
2. Rename it to `rsplayer` and make it executable:

```bash
chmod +x rsplayer
./rsplayer
```

**Option 2 — Desktop app:**

1. Download the `.dmg` from the [latest release](https://github.com/ljufa/rsplayer/releases/latest).
2. Open the DMG and drag RSPlayer to your Applications folder.

?> On macOS, network mount management, Linux power actions, and firmware USB integration are unavailable.

## Verify installation
* Run systemd service by `sudo systemctl start rsplayer`
* Check service status by `sudo systemctl status rsplayer` and if it shows active go to the next step
* Open browser at http://you-machine-ip-address i.e. http://raspberrypi.local.

?>TIP: The HTTP and HTTPS ports are configured in the `/opt/rsplayer/env` file. By default, `PORT` is set to 80 and `TLS_PORT` is set to 443. You can edit this file to change the ports used by `rsplayer`.
* If the page can not load or there is an error message at top of the page please see the [Troubleshooting](troubleshooting.md) section.
