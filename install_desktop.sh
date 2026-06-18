#!/usr/bin/env bash
set -e

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Install the RSPlayer desktop application.

Options:
  -p, --pre-release  Download latest pre-release instead of stable
  -h, --help         Show this help message

Supported platforms:
  Linux   x86_64 — .deb (Debian/Ubuntu) or .rpm (Fedora/RHEL/openSUSE)
  macOS   Intel / Apple Silicon — .dmg

For ARM, RISC-V, or headless/server installs, use install.sh instead:
  bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
EOF
    exit 0
}

PRE_RELEASE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        -p|--pre-release) PRE_RELEASE=true ;;
        -h|--help) usage ;;
        *) echo "Unknown option: $1"; usage ;;
    esac
    shift
done

echo "========================================"
echo "RSPlayer Desktop Installer"
echo "========================================"

# ── OS detection ────────────────────────────
os_type=$(uname -s)
device_arch=$(uname -m)
echo "[INFO] OS: $os_type, architecture: $device_arch"

if [ "$os_type" = "Linux" ]; then
    # ── Linux: x86_64 only ──────────────────
    if [ "$device_arch" != "x86_64" ]; then
        echo "[ERROR] The RSPlayer desktop app is only available for x86_64 Linux."
        echo "[INFO]  For $device_arch devices, use the headless server installer:"
        echo "        bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)"
        exit 1
    fi

    # Use sudo only if not already root
    if [ "$(id -u)" -eq 0 ]; then
        SUDO=""
    else
        SUDO="sudo"
    fi

    # Detect distribution
    echo "[INFO] Detecting distribution..."
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo "[INFO] OS: $PRETTY_NAME (ID=$ID)"
        case $ID in
            debian|ubuntu|raspbian|linuxmint|pop|zorin|elementary)
                pkg_type="deb"
                pkg_ext="deb"
                pkg_pattern="rsplayer-desktop_.*_amd64\\.deb"
                ;;
            fedora|rhel|centos|opensuse*|rocky|alma)
                pkg_type="rpm"
                pkg_ext="rpm"
                pkg_pattern="rsplayer-desktop-.*\\.x86_64\\.rpm"
                ;;
            *)
                echo "[WARN] Unknown distribution '$ID', defaulting to deb"
                pkg_type="deb"
                pkg_ext="deb"
                pkg_pattern="rsplayer-desktop_.*_amd64\\.deb"
                ;;
        esac
    else
        echo "[WARN] /etc/os-release not found, assuming Debian-based"
        pkg_type="deb"
        pkg_ext="deb"
        pkg_pattern="rsplayer-desktop_.*_amd64\\.deb"
    fi

    echo "[INFO] Selected package type: $pkg_type"

    # ── Download ─────────────────────────────
    if [ "$PRE_RELEASE" = true ]; then
        echo "[INFO] Querying GitHub API for latest pre-release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases?per_page=1" \
            | grep browser_download_url | cut -d '"' -f 4 | grep -E "$pkg_pattern" | head -n 1)
    else
        echo "[INFO] Querying GitHub API for latest stable release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases/latest" \
            | grep browser_download_url | cut -d '"' -f 4 | grep -E "$pkg_pattern" | head -n 1)
    fi

    if [ -z "$URL" ]; then
        echo "[ERROR] No $pkg_type package found for rsplayer-desktop on x86_64"
        exit 1
    fi

    pkg_file_name=$(basename "$URL")
    echo "[INFO] Downloading: $URL"
    if ! curl -L -o "$pkg_file_name" "$URL"; then
        echo "[ERROR] Download failed"
        rm -f "$pkg_file_name"
        exit 1
    fi
    echo "[INFO] Download OK: $pkg_file_name ($(du -h "$pkg_file_name" | cut -f1))"

    # ── Install ──────────────────────────────
    echo "[INFO] Installing $pkg_type package: $pkg_file_name"
    case $pkg_type in
        deb)
            echo "[INFO] Installing deb with apt-get (resolves dependencies automatically)"
            $SUDO apt-get update
            $SUDO apt-get install -y "./${pkg_file_name}" || {
                echo "[INFO] Retrying with dpkg + apt-get -f install to resolve dependencies"
                $SUDO dpkg -i --force-overwrite "./${pkg_file_name}" || true
                $SUDO apt-get install -f -y
            }
            ;;
        rpm)
            if command -v dnf &>/dev/null; then
                echo "[INFO] Installing rpm with dnf"
                $SUDO dnf install -y "./${pkg_file_name}"
            elif command -v zypper &>/dev/null; then
                echo "[INFO] Installing rpm with zypper"
                $SUDO zypper install -y "./${pkg_file_name}"
            else
                echo "[INFO] Installing rpm with rpm"
                $SUDO rpm -i "./${pkg_file_name}" || {
                    echo "[INFO] Retrying with dnf (may resolve dependencies)"
                    $SUDO dnf install -y "./${pkg_file_name}" 2>/dev/null || true
                }
            fi
            ;;
    esac

    rm -f "$pkg_file_name"

    echo ""
    echo "========================================"
    echo "Desktop app installed successfully."
    echo "Launch it from your application menu,"
    echo "or run:  rsplayer-desktop"
    echo "========================================"

elif [ "$os_type" = "Darwin" ]; then
    # ── macOS: DMG ───────────────────────────
    if [ "$device_arch" = "arm64" ]; then
        arch_suffix="aarch64"
    elif [ "$device_arch" = "x86_64" ]; then
        arch_suffix="x64"
    else
        echo "[ERROR] Unsupported macOS architecture: $device_arch"
        exit 1
    fi

    pkg_pattern="rsplayer-desktop_.*_${arch_suffix}\\.dmg"

    # ── Download ─────────────────────────────
    if [ "$PRE_RELEASE" = true ]; then
        echo "[INFO] Querying GitHub API for latest pre-release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases?per_page=1" \
            | grep browser_download_url | cut -d '"' -f 4 | grep -E "$pkg_pattern" | head -n 1)
    else
        echo "[INFO] Querying GitHub API for latest stable release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases/latest" \
            | grep browser_download_url | cut -d '"' -f 4 | grep -E "$pkg_pattern" | head -n 1)
    fi

    if [ -z "$URL" ]; then
        echo "[ERROR] No DMG found for macOS $device_arch"
        exit 1
    fi

    dmg_file=$(basename "$URL")
    echo "[INFO] Downloading: $URL"
    if ! curl -L -o "$dmg_file" "$URL"; then
        echo "[ERROR] Download failed"
        rm -f "$dmg_file"
        exit 1
    fi
    echo "[INFO] Download OK: $dmg_file ($(du -h "$dmg_file" | cut -f1))"

    # ── Mount and install ────────────────────
    echo "[INFO] Mounting DMG..."
    mount_output=$(hdiutil attach "$dmg_file" -nobrowse)
    mount_point=$(echo "$mount_output" | grep -o '/Volumes/.*' | head -n 1)

    if [ -z "$mount_point" ]; then
        echo "[ERROR] Failed to mount DMG"
        rm -f "$dmg_file"
        exit 1
    fi
    echo "[INFO] Mounted at: $mount_point"

    # Find the .app bundle
    app_path=$(find "$mount_point" -maxdepth 2 -name "*.app" -type d | head -n 1)
    if [ -z "$app_path" ]; then
        echo "[ERROR] No .app bundle found in DMG"
        hdiutil detach "$mount_point" -quiet
        rm -f "$dmg_file"
        exit 1
    fi

    app_name=$(basename "$app_path")
    echo "[INFO] Installing $app_name to /Applications..."

    # Remove existing version if present
    if [ -d "/Applications/$app_name" ]; then
        echo "[INFO] Removing existing /Applications/$app_name"
        rm -rf "/Applications/$app_name"
    fi

    cp -R "$app_path" /Applications/

    # Unmount and clean up
    echo "[INFO] Unmounting DMG..."
    hdiutil detach "$mount_point" -quiet
    rm -f "$dmg_file"

    echo ""
    echo "========================================"
    echo "Desktop app installed successfully."
    echo "Launch it from /Applications/$app_name,"
    echo "or run:  open /Applications/$app_name"
    echo "========================================"

else
    echo "[ERROR] Unsupported operating system: $os_type"
    echo "[INFO]  RSPlayer desktop is available for Linux (x86_64) and macOS."
    echo "[INFO]  For the headless server on other platforms, use:"
    echo "        bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)"
    exit 1
fi
