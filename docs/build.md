# Local Build Guide

This document describes how to build `rsplayer` from source — on Linux (including cross-compilation for Linux/macOS targets) and natively on Windows.

Common cross-compilation targets (Linux host):

- `arm-unknown-linux-gnueabihf` (ARMv6)
- `armv7-unknown-linux-gnueabihf` (ARMv7)
- `aarch64-unknown-linux-gnu` (ARM64 Linux)
- `x86_64-unknown-linux-gnu`
- `riscv64gc-unknown-linux-gnu`
- `aarch64-apple-darwin` (macOS Apple Silicon, experimental)
- `x86_64-apple-darwin` (macOS Intel, experimental)

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

`npm install` automatically runs a `postinstall` script that copies FontAwesome and Material Icons font files from `node_modules/` into `public/`. These directories are gitignored — do not commit them.

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

For macOS builds from Linux, use one of the darwin targets:

```bash
TARGET=aarch64-apple-darwin cargo make build_release
# or
TARGET=x86_64-apple-darwin cargo make build_release
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

### Build headless server

The Windows server binary is built without ALSA or LIRC features (those are Linux-only):

```powershell
# Build the web UI first (required — embedded into the server binary)
cargo make build_ui_release

# Build the headless server
cargo build --package rsplayer --bin rsplayer --release --no-default-features --target x86_64-pc-windows-msvc
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

### Platform limitations on Windows

- **Volume control**: software gain only (ALSA and PipeWire are unavailable).
- **Network mounts**: SMB/NFS mounting from the UI is unavailable.
- **Power control**: system poweroff/reboot commands are unavailable.
- **IR remote**: LIRC integration is unavailable.
- **Firmware USB**: serial integration works (cross-platform via `serialport` crate).

## Output

After a successful build, Linux packages and binaries are located under the target output directories:

`target/${TARGET}/debian/`
`target/${TARGET}/release/`

When local `cargo-make` cross target-dir override is active, artifacts are under:

`target/cross/${TARGET}/release/`

For example: `target/aarch64-unknown-linux-gnu/debian/rsplayer_1.0.3_arm64.deb`
