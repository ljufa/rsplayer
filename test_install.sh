#!/bin/bash
# Launch a Docker container with systemd for a given distro and run install.sh.
#
# Uses antmelekhin/docker-systemd images which have systemd pre-configured.
# systemctl, journalctl etc. work out of the box.
#
# Native arch:  Mounts local install.sh into the container so you can
#               edit and re-run without pushing to GitHub.
# Cross-arch:   Downloads install.sh from GitHub (needs curl in container).
#
# Prerequisites:
#   - Docker
#   - For cross-arch: docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
#
# Supported platforms (antmelekhin/docker-systemd only provides amd64 and arm64):
#   - linux/amd64
#   - linux/arm64
#   - linux/riscv64
#
# Usage:
#   ./test_install.sh <distro> [platform]
#
# Examples:
#   ./test_install.sh debian                  # Debian 12 on native arch
#   ./test_install.sh fedora                  # Fedora 40 on native arch
#   ./test_install.sh arch                    # Arch Linux on native arch (no systemd)
#   ./test_install.sh debian linux/arm64      # Debian 12 on aarch64
#   ./test_install.sh fedora linux/arm64      # Fedora 40 on aarch64
#   ./test_install.sh debian linux/riscv64    # Debian 12 on riscv64

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DISTRO="${1:-}"
PLATFORM="${2:-}"
CONTAINER_NAME="rsplayer-test-${DISTRO:-none}"

if [ -z "$DISTRO" ]; then
    echo "Usage: $0 <debian|fedora|arch> [linux/amd64|linux/arm64|linux/riscv64]"
    echo
    echo "Supported distros:   debian, fedora, arch"
    echo "Supported platforms: linux/amd64, linux/arm64, linux/riscv64 (default: native)"
    echo
    echo "Note: arm/v7 and arm/v6 are NOT supported by the systemd docker images."
    echo "      Arch Linux runs without systemd (systemctl won't work)."
    echo
    echo "Native arch:  mounts local install.sh (edit & re-run inside container)"
    echo "Cross-arch:   fetches install.sh from GitHub"
    exit 1
fi

# Validate platform
if [ -n "$PLATFORM" ] && [ "$PLATFORM" != "linux/amd64" ] && [ "$PLATFORM" != "linux/arm64" ] && [ "$PLATFORM" != "linux/riscv64" ]; then
    echo "ERROR: Unsupported platform '$PLATFORM'"
    echo "Only linux/amd64, linux/arm64, and linux/riscv64 are supported."
    exit 1
fi

PLATFORM_FLAG=""
if [ -n "$PLATFORM" ]; then
    PLATFORM_FLAG="--platform $PLATFORM"
fi

# antmelekhin/docker-systemd images have systemd as PID 1
# Arch not available there, so we use plain archlinux image (no systemd as PID 1)
case "$DISTRO" in
    debian)
        IMAGE="antmelekhin/docker-systemd:debian-12"
        INSTALL_DEPS="apt-get update && apt-get install -y curl"
        # INSTALL_DEPS=""
        ;;
    fedora)
        IMAGE="antmelekhin/docker-systemd:fedora-43"
        INSTALL_DEPS="dnf install -y curl"
        # INSTALL_DEPS=""
        ;;
    arch)
        IMAGE="carlodepieri/docker-archlinux-systemd:latest"
        INSTALL_DEPS="pacman -Syu --noconfirm curl"
        if [ -n "$PLATFORM" ] && [ "$PLATFORM" != "linux/amd64" ]; then
            echo "ERROR: Arch systemd image only supports linux/amd64"
            exit 1
        fi
        ;;
    *)
        echo "Unknown distro: $DISTRO (use debian, fedora, or arch)"
        exit 1
        ;;
esac

# Clean up any previous container with the same name
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true

echo "========================================"
echo "RSPlayer Install Test"
echo "========================================"
echo "Distro:    $DISTRO ($IMAGE)"
echo "Platform:  ${PLATFORM:-native}"
echo "Container: $CONTAINER_NAME"
echo "UI:        http://localhost:8010 (after install)"
echo "========================================"

MOUNT_FLAG="-v ${SCRIPT_DIR}/install.sh:/mnt/install.sh:ro"
echo "Mode: LOCAL (install.sh mounted at /mnt/install.sh)"
INSTALL_CMD="bash /mnt/install.sh"

echo
echo "Starting container with systemd..."
echo "========================================"

# Start container with systemd as PID 1
docker run -d \
    $PLATFORM_FLAG \
    --name "$CONTAINER_NAME" \
    --privileged \
    --cgroupns=host \
    -p 8010:80 \
    $MOUNT_FLAG \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
    "$IMAGE"

echo "Waiting for systemd to boot..."
sleep 3

if [ -n "$PLATFORM" ]; then
    echo
    echo "[WARN] Cross-arch via QEMU emulation — everything will be SLOW."
    echo "[WARN] Package installs (dnf/apt) can take 5-15 minutes."
    echo "[WARN] For faster testing, use real arm64/riscv64 hardware."
    echo
    echo "Skipping dependency install from host (too slow)."
    echo "Install deps manually inside the container."
else
    echo "Installing dependencies..."
    docker exec "$CONTAINER_NAME" bash -c "$INSTALL_DEPS"
fi

echo
echo "========================================"
echo "Container ready. Entering shell."
echo "========================================"
echo
if [ -n "$PLATFORM" ]; then
    echo "1. Install deps first:"
    echo "   $INSTALL_DEPS"
    echo
    echo "2. Run installer:"
    echo "   $INSTALL_CMD"
else
    echo "Run this to install rsplayer:"
    echo "  $INSTALL_CMD"
fi
echo
echo "After install:"
echo "  systemctl status rsplayer"
echo "  journalctl -u rsplayer -f -n 50"
echo
echo "UI: http://localhost:8010"
echo "========================================"
echo

# Exec into the container
docker exec -it "$CONTAINER_NAME" bash

echo
echo "========================================"
echo "Exited shell. Container is still running."
echo "  Re-enter:  docker exec -it $CONTAINER_NAME bash"
echo "  Stop:      docker rm -f $CONTAINER_NAME"
echo "========================================"
