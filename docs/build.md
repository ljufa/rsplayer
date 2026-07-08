# Local Build Guide

This document describes how to build `rsplayer` from source — on Linux (including cross-compilation for other Linux architectures), natively on macOS, and natively on Windows.

Common cross-compilation targets (Linux host):

- `arm-unknown-linux-gnueabihf` (ARMv6)
- `armv7-unknown-linux-gnueabihf` (ARMv7)
- `aarch64-unknown-linux-gnu` (ARM64 Linux)
- `x86_64-unknown-linux-gnu`
- `riscv64gc-unknown-linux-gnu`

## Prerequisites

Before you begin, ensure you have the following prerequisites installed on your Linux development machine.

### 1. Rust and dev tools
Install linux build tools:
```
sudo apt install build-essential pkg-config libasound2-dev
```
Install Rust using `rustup`:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Docker or Podman

The `cross` tool uses a container engine to manage the cross-compilation environment. You must have Docker or Podman installed and running.

- **Docker:** [Install Docker Engine](https://docs.docker.com/engine/install/)
- **Podman:** [Install Podman](https://podman.io/getting-started/installation)

### 3. Cargo Build Tools

Install the necessary cargo tools for building the application and Linux packages:
```bash
cargo install cross
cargo install cargo-deb
cargo install cargo-make
```

### 4. Dioxus CLI

Install the Dioxus CLI for building the frontend:
```bash
cargo install dioxus-cli
```

### 5. Node.js and npm

Required for frontend CSS compilation and font asset setup:
```bash
# Install Node.js via your package manager, e.g.:
sudo apt install nodejs npm
```

## Frontend Setup (one-time after clone)

The frontend lives in `web-ui/`. Run once after cloning to install npm packages and copy font assets to `public/`:

```bash
cd web-ui
npm install
```

`npm install` automatically runs a `postinstall` script that copies the Material Icons font files from `node_modules/` into `public/`. These directories are gitignored — do not commit them.

## Build Process

### Frontend (Web UI)

The frontend is a Rust/WASM app built with the Dioxus framework.

All UI build commands run via `cargo make` from the repo root:

**Development** — served with hot-reload, proxies `/api` and `/artwork` to backend on `localhost:8000`:
```bash
cargo make serve_dev
```
Or manually: `cd web-ui && dx serve`

**Release** — produces a self-contained output under `dist/web-ui/`, embedded into the backend binary at compile time:
```bash
cargo make build_ui_release
```

**Updating CSS** — only needed when `input.css` is changed (Tailwind source):
```bash
cargo make build_css
# commit public/tw.css after regenerating
```

### Backend

The build is orchestrated using `cargo-make`. Build the frontend release first, then the backend.

1. **Set the target architecture** in `Makefile.toml` (or pass it via env). For example, a 64-bit Raspberry Pi 4 target:
    ```
    TARGET="aarch64-unknown-linux-gnu"
    ```

2. **Run the build:**
    ```bash
    # Build the frontend (must be done before backend)
    cargo make build_ui_release

    # Build the backend application using cross-compilation
    cargo make build_release

    # Copy binary to target device (Optional, Linux targets)
    # Adjust TARGET_HOST=xxx.xxx.xxx.xx in Makefile.toml
    cargo make copy_remote

    # Create the .deb package (Optional)
    cargo make package_deb_release
    ```

### macOS targets

macOS binaries are built **natively on a Mac** (cross-compiling from Linux is not supported — the osxcross-based pipeline was removed in v4.0.0; CI uses GitHub macOS runners). On a Mac with Rust, `cargo-make`, and the Dioxus CLI installed:

```bash
cargo make build_ui_release
cargo build --package rsplayer --bin rsplayer --release --no-default-features
```

Darwin release output is binary-only (no `.deb`, `.rpm`, `.tgz` packaging).

## Windows Build (native)

Windows builds must be compiled natively on a Windows machine (or a Windows GitHub Actions runner). Cross-compiling from Linux to Windows is not supported.

### Prerequisites

1. **Rust** — install from [rustup.rs](https://rustup.rs/). The MSVC toolchain is selected by default on Windows.
2. **Visual Studio Build Tools** or Visual Studio with the "Desktop development with C++" workload (needed by some C dependencies).
3. **cargo-make** and **tauri-cli** (for the desktop app):
   ```powershell
   cargo install cargo-make
   cargo install tauri-cli --version "^2"
   ```
4. **Node.js** — required for the web UI CSS build step.
5. **LLVM** — required for `bindgen` (ASIO). Install from [releases.llvm.org](https://releases.llvm.org/) or `choco install llvm`, then set `LIBCLANG_PATH` to its `bin` directory (e.g. `C:\Program Files\LLVM\bin`).
6. **ASIO SDK** — required to link ASIO output. Download the [Steinberg ASIO SDK](https://www.steinberg.net/asiosdk), extract it, and set `CPAL_ASIO_DIR` to the extracted folder. The SDK is not redistributed with rsplayer; you accept Steinberg's license by downloading it.

### Build headless server

The Windows server binary is built without ALSA or LIRC (Linux-only) but **with** the `asio` feature for low-latency ASIO output:

```powershell
# Build the web UI first (required — embedded into the server binary)
cargo make build_ui_release

# Point the build at the ASIO SDK and LLVM (once per shell)
$env:CPAL_ASIO_DIR = "C:\path\to\asiosdk"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"

# Build the headless server
cargo build --package rsplayer --bin rsplayer --release --no-default-features --features asio --target x86_64-pc-windows-msvc
```

The binary is at `target\x86_64-pc-windows-msvc\release\rsplayer.exe`.

### Build desktop app

```powershell
# Copy loading.html alongside the web UI dist
copy crates\desktop\loading.html dist\web-ui\loading.html

# Build and bundle as NSIS installer
cd crates\desktop
cargo tauri build --bundles nsis --ci
```

The NSIS installer is placed under `target\release\bundle\nsis\`.

The desktop app links ASIO too (via the desktop crate's Windows dependency); the same `CPAL_ASIO_DIR` and `LIBCLANG_PATH` must be set before `cargo tauri build`.

### Audio output on Windows

- **WASAPI** (default host) devices are always available.
- **ASIO** drivers appear as `… (ASIO)` entries in Settings → Audio interface when the build includes the `asio` feature and an ASIO driver is installed. ASIO gives exclusive, low-latency, bit-perfect output; the driver's own control panel sets its buffer size and sample rate.

> ASIO is a trademark and software of Steinberg Media Technologies GmbH.

### Platform limitations on Windows

- **Volume control**: software gain only (ALSA and PipeWire are unavailable). Software gain still applies to ASIO output.
- **Network mounts**: SMB/NFS mounting from the UI is unavailable.
- **Power control**: system poweroff/reboot commands are unavailable.
- **IR remote**: LIRC integration is unavailable.
- **Firmware USB**: serial integration works (cross-platform via `serialport` crate).

### Self-hosted Windows runner (CI)

The `build_desktop_windows` job runs on a self-hosted runner (`runs-on: [self-hosted, windows, x64]`). Unlike GitHub-hosted `windows-latest`, a bare machine has **nothing** pre-installed, so the runner will fail immediately (e.g. `bash: command not found`, because `dtolnay/rust-toolchain` is a bash action). Provision the machine **once**, in an elevated PowerShell:

```powershell
# Allow the runner to execute the per-step .ps1 files it generates.
# Windows defaults to Restricted, which blocks them ("running scripts is disabled").
# Elevated shell (runner as a service): use -Scope LocalMachine.
# Non-elevated shell (runner runs as the current user): use -Scope CurrentUser.
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force

# Toolchain + native build prerequisites
winget install --id Git.Git -e                     # git (checkout) + Git Bash
winget install --id Rustlang.Rustup -e             # rustup / cargo
winget install --id LLVM.LLVM -e                   # libclang for bindgen (ASIO)
winget install --id Microsoft.VisualStudio.2022.BuildTools -e `
  --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

# Rust toolchain + MSVC target
rustup toolchain install stable
rustup target add x86_64-pc-windows-msvc

# Optional: pre-install so it isn't rebuilt every run
cargo install tauri-cli --version "^2" --locked
```

Notes:

- The workflow sets `LIBCLANG_PATH: C:\Program Files\LLVM\bin`, which matches the default LLVM install location. Adjust the env in `cd.yml` if you install LLVM elsewhere.
- **Visual Studio Build Tools** (the "Desktop development with C++" / VCTools workload) provides the MSVC linker and Windows SDK needed to compile the ASIO C++ shim and link the executables.
- **Restart the runner service after provisioning** — the runner captures `PATH`/environment when it starts, so newly installed `cargo`, `rustup`, `clang` and `git` are not visible until it restarts.
- The workflow's Windows steps use the built-in Windows PowerShell (`shell: powershell`), so neither Git Bash nor PowerShell 7 (`pwsh`) needs to be installed. Git itself is still needed for `actions/checkout`.

## Flatpak (desktop app)

The desktop app is packaged for Flathub from `PKGS/flatpak/` — see
[`PKGS/flatpak/README.md`](https://github.com/ljufa/rsplayer/blob/master/PKGS/flatpak/README.md)
for regenerating `cargo-sources.json`, building locally with `flatpak-builder`,
and the per-release update checklist. The manifest builds the workspace offline
from vendored crates plus the `web-ui-dist-<version>.tar.gz` release asset
published by the "Full release" workflow.

## Output

After a successful build, Linux packages and binaries are located under the target output directories:

`target/${TARGET}/debian/`
`target/${TARGET}/release/`

When local `cargo-make` cross target-dir override is active, artifacts are under:

`target/cross/${TARGET}/release/`

For example: `target/aarch64-unknown-linux-gnu/debian/rsplayer_<version>_arm64.deb`
