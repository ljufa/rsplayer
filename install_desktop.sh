#!/usr/bin/env bash
set -e

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  -p, --pre-release  Download latest pre-release instead of stable
  -h, --help         Show this help message
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

if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    SUDO="sudo"
fi

device_arch=$(uname -m)
echo "[INFO] Device architecture: $device_arch"

if [ "$device_arch" = "x86_64" ]; then
    deb_arch_suffix="amd64"
    rpm_arch_suffix="x86_64"
    arch_arch_suffix="amd64"
elif [ "$device_arch" = "aarch64" ]; then
    deb_arch_suffix="arm64"
    rpm_arch_suffix="aarch64"
    arch_arch_suffix="arm64"
elif [ "$device_arch" = "armv7l" ]; then
    deb_arch_suffix="armhfv7"
    rpm_arch_suffix="armv7hl"
    arch_arch_suffix="armhfv7"
elif [ "$device_arch" = "armv6l" ]; then
    deb_arch_suffix="armhfv6"
    rpm_arch_suffix="armv6hl"
    arch_arch_suffix="armhfv6"
elif [ "$device_arch" = "riscv64" ]; then
    deb_arch_suffix="riscv64"
    rpm_arch_suffix="riscv64"
    arch_arch_suffix="riscv64"
else
    deb_arch_suffix=$device_arch
    rpm_arch_suffix=$device_arch
    arch_arch_suffix=$device_arch
fi

echo "[INFO] Architecture suffixes: deb=$deb_arch_suffix rpm=$rpm_arch_suffix arch=$arch_arch_suffix"

echo "[INFO] Detecting distribution..."
if [ -f /etc/os-release ]; then
    . /etc/os-release
    echo "[INFO] OS: $PRETTY_NAME (ID=$ID)"
    case $ID in
        debian|ubuntu|raspbian)
            pkg_type="deb"
            pkg_suffix="$deb_arch_suffix"
            pkg_ext="deb"
            ;;
        fedora|rhel|centos|opensuse*)
            pkg_type="rpm"
            pkg_suffix="$rpm_arch_suffix"
            pkg_ext="rpm"
            ;;
        arch|archarm|manjaro)
            pkg_type="arch"
            pkg_suffix="$arch_arch_suffix"
            pkg_ext="tgz"
            ;;
        *)
            echo "[WARN] Unknown distribution '$ID', defaulting to deb"
            pkg_type="deb"
            pkg_suffix="$deb_arch_suffix"
            pkg_ext="deb"
            ;;
    esac
else
    echo "[WARN] /etc/os-release not found, assuming Debian-based"
    pkg_type="deb"
    pkg_suffix="$deb_arch_suffix"
    pkg_ext="deb"
fi

echo "[INFO] Selected package type: $pkg_type (suffix=$pkg_suffix, ext=$pkg_ext)"

try_download() {
    local type=$1
    local suffix=$2
    local ext=$3
    local file="rsplayer-desktop_${suffix}.${ext}"

    if [ "$PRE_RELEASE" = true ]; then
        echo "[INFO] Querying GitHub API for latest pre-release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases?per_page=1" | grep browser_download_url | cut -d '"' -f 4 | grep "/rsplayer-desktop" | grep "[-_.]${suffix}\.${ext}" | head -n 1)
    else
        echo "[INFO] Querying GitHub API for latest stable release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases/latest" | grep browser_download_url | cut -d '"' -f 4 | grep "/rsplayer-desktop" | grep "[-_.]${suffix}\.${ext}" | head -n 1)
    fi
    if [ -z "$URL" ]; then
        echo "[WARN] No $type desktop package found for suffix=$suffix ext=$ext"
        return 1
    fi
    echo "[INFO] Downloading: $URL"
    if curl -L -o "$file" "$URL"; then
        echo "[INFO] Download OK: $file ($(du -h "$file" | cut -f1))"
        pkg_type="$type"
        pkg_suffix="$suffix"
        pkg_ext="$ext"
        pkg_file_name="$file"
        return 0
    else
        echo "[ERROR] Download failed"
        rm -f "$file"
        return 1
    fi
}

echo "[INFO] Attempting primary package type: $pkg_type"
if ! try_download "$pkg_type" "$pkg_suffix" "$pkg_ext"; then
    echo "[WARN] Primary package type $pkg_type not available, falling back to DEB"
    if ! try_download "deb" "$deb_arch_suffix" "deb"; then
        echo "[ERROR] No suitable desktop package found for architecture $device_arch"
        exit 1
    fi
fi

echo "[INFO] Installing $pkg_type desktop package: $pkg_file_name"
case $pkg_type in
    deb)
        echo "[INFO] Installing deb with apt-get (resolves dependencies automatically)"
        $SUDO apt-get update
        $SUDO apt-get install "./${pkg_file_name}" || {
            echo "[INFO] Retrying with dpkg + apt-get -f install to resolve dependencies"
            $SUDO dpkg -i --force-overwrite "./${pkg_file_name}" || true
            $SUDO apt-get install -f
        }
        ;;
    rpm)
        echo "[INFO] Installing rpm with dnf (resolves dependencies automatically)"
        $SUDO dnf install "./${pkg_file_name}"
        ;;
    arch)
        echo "[INFO] Installing desktop dependencies..."
        $SUDO pacman -S --needed webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg alsa-lib
        echo "[INFO] Extracting tarball to / (files go to /usr/bin, /usr/share)"
        $SUDO tar -xzvf "${pkg_file_name}" -C /
        echo "[INFO] Updating icon cache..."
        $SUDO gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor 2>/dev/null || true
        ;;
esac

rm "${pkg_file_name}"

echo "========================================"
echo "[INFO] Desktop installation complete!"
echo "[INFO] Package type: $pkg_type"
echo "[INFO] Architecture: $device_arch ($pkg_suffix)"
echo "========================================"
echo "[INFO] You can launch RSPlayer from your application menu"
echo "       or by running 'rsplayer-desktop' in a terminal"
echo "========================================"
