# Local Linux Cross-Build for Raspberry Pi

This document provides instructions on how to perform a local cross-build of the `rsplayer` application for a Raspberry Pi 4 (64-bit) on a Linux host machine.

The primary target for Raspberry Pi 4 is `aarch64-unknown-linux-gnu`.

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

Install the necessary cargo tools for building the application and Debian package:
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

The frontend lives in `rsplayer_web_ui/`. Run once after cloning to install npm packages and copy font assets to `public/`:

```bash
cd rsplayer_web_ui
npm install
```

`npm install` automatically runs a `postinstall` script that copies FontAwesome and Material Icons font files from `node_modules/` into `public/`. These directories are gitignored — do not commit them.

## Build Process

### Frontend (Web UI)

The frontend is a Rust/WASM app built with the Dioxus framework.

**Development** — served with hot-reload, proxies `/api` and `/artwork` to the backend on `localhost:8000`:
```bash
cd rsplayer_web_ui
dx serve
```

**Release** — produces a self-contained output under `target/dx/rsplayer_web_ui/release/web/public/`, which is embedded into the backend binary at compile time:
```bash
cd rsplayer_web_ui
dx build --release --platform web
```

**Updating CSS** — only needed when `input.css` is changed (Tailwind source):
```bash
cd rsplayer_web_ui
npx tailwindcss -i input.css -o public/tw.css --minify
# commit public/tw.css after regenerating
```

### Backend

The build is orchestrated using `cargo-make`. Build the frontend release first, then the backend.

1. **Set the target architecture** in `Makefile.toml`. Possible values are listed in `.cargo/config.toml`. For a 64-bit Raspberry Pi 4:
    ```
    TARGET="aarch64-unknown-linux-gnu"
    ```

2. **Run the build:**
    ```bash
    # Build the frontend (must be done before backend)
    cd rsplayer_web_ui && dx build --release --platform web && cd ..

    # Build the backend application using cross-compilation
    cargo make build_release

    # Copy binary to target device (Optional)
    # Adjust RPI_HOST=xxx.xxx.xxx.xx (local ip)
    cargo make copy_remote

    # Create the .deb package (Optional)
    cargo make package_deb_release
    ```

## Output

After a successful build, the Debian package will be located in the following directory:

`target/${TARGET}/debian/`
`target/${TARGET}/release/`

For example: `target/aarch64-unknown-linux-gnu/debian/rsplayer_1.0.3_arm64.deb`
