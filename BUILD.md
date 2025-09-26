# Local Linux Cross-Build for Raspberry Pi

This document provides instructions on how to perform a local cross-build of the `rsplayer` application for a Raspberry Pi 4 (64-bit) on a Linux host machine.

The primary target for Raspberry Pi 4 is `aarch64-unknown-linux-gnu`.

## Prerequisites

Before you begin, ensure you have the following prerequisites installed on your Linux development machine.

### 1. Rust and dev tools
Install linux build tools:
```
sudo apt install build-essintials pkg-config libasound2-dev
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

Install the necessary cargo tools for building the application, UI, and Debian package:
```bash
cargo install cross
cargo install cargo-deb
cargo install wasm-pack
cargo install cargo-make
```

## Build Process

The build is orchestrated using `cargo-make`. The process involves building the WebAssembly UI first, followed by the backend application, and finally packaging everything into a `.deb` file.

1.  **Set the Target Environment Variable:**
    Export the target architecture for the build. For a 64-bit Raspberry Pi 4, use `aarch64-unknown-linux-gnu`.
    Set target value in `Makefile.toml`. Possible values are listed in `.cargo/config.toml` file.
    ```bash
    TARGET="aarch64-unknown-linux-gnu"
    ```

2.  **Run the Build:**
    Execute the following commands to build the UI, the backend, and create the Debian package.

    ```bash
    # Build the UI components (WASM)
    cargo make build_ui_release

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
