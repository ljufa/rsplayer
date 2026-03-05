#!/usr/bin/env bash
set -ex

echo "========================================"
echo "RSPlayer Installer"
echo "========================================"

# Use sudo only if not already root
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    SUDO="sudo"
fi

device_arch=$(uname -m)
echo "[INFO] Device architecture: $device_arch"

# Map device architecture to package suffix per distribution
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
else
    deb_arch_suffix=$device_arch
    rpm_arch_suffix=$device_arch
    arch_arch_suffix=$device_arch
fi

echo "[INFO] Architecture suffixes: deb=$deb_arch_suffix rpm=$rpm_arch_suffix arch=$arch_arch_suffix"

# Detect distribution
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

# Function to attempt download
try_download() {
    local type=$1
    local suffix=$2
    local ext=$3
    local file="rsplayer_${suffix}.${ext}"

    echo "[INFO] Querying GitHub API for latest release..."
    URL=$(curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | cut -d '"' -f 4 | grep "_${suffix}.${ext}" | head -n 1)
    if [ -z "$URL" ]; then
        echo "[WARN] No $type package found for suffix=$suffix ext=$ext"
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

# Try primary package type
echo "[INFO] Attempting primary package type: $pkg_type"
if ! try_download "$pkg_type" "$pkg_suffix" "$pkg_ext"; then
    echo "[WARN] Primary package type $pkg_type not available, falling back to DEB"
    if ! try_download "deb" "$deb_arch_suffix" "deb"; then
        echo "[ERROR] No suitable package found for architecture $device_arch"
        exit 1
    fi
fi

echo "[INFO] Installing $pkg_type package: $pkg_file_name"
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
        echo "[INFO] Extracting tarball to / (files go to /usr/bin, /etc, /opt/rsplayer)"
        $SUDO tar -xzvf "${pkg_file_name}" -C /
        echo "[INFO] Creating groups..."
        getent group audio >/dev/null || $SUDO groupadd -r audio
        getent group dialout >/dev/null || $SUDO groupadd -r dialout
        getent group i2c >/dev/null || $SUDO groupadd -r i2c
        getent group gpio >/dev/null || $SUDO groupadd -r gpio
        getent group spi >/dev/null || $SUDO groupadd -r spi
        getent group input >/dev/null || $SUDO groupadd -r input
        getent group rsplayer >/dev/null || $SUDO groupadd -r rsplayer
        echo "[INFO] Creating rsplayer user..."
        if ! getent passwd rsplayer >/dev/null; then
            $SUDO useradd -r -s /bin/false -d /opt/rsplayer -g rsplayer rsplayer || :
            $SUDO usermod -a -G audio,dialout,i2c,gpio,spi,input rsplayer || true
        else
            echo "[INFO] rsplayer user already exists"
        fi
        echo "[INFO] Setting ownership on /opt/rsplayer"
        $SUDO chown -R rsplayer /opt/rsplayer
        echo "[INFO] Enabling and starting systemd service"
        $SUDO systemctl daemon-reload
        $SUDO systemctl enable rsplayer
        $SUDO systemctl start rsplayer
        ;;
esac

rm "${pkg_file_name}"

# Ensure /opt/rsplayer is owned by rsplayer regardless of package type
echo "[INFO] Ensuring /opt/rsplayer ownership..."
if getent passwd rsplayer >/dev/null 2>&1; then
    $SUDO chown -R rsplayer /opt/rsplayer
    echo "[INFO] /opt/rsplayer owned by rsplayer:rsplayer"
else
    echo "[WARN] rsplayer user not found, skipping chown"
fi

echo "========================================"
echo "[INFO] Installation complete!"
echo "[INFO] Package type: $pkg_type"
echo "[INFO] Architecture: $device_arch ($pkg_suffix)"
echo "========================================"
echo "[INFO] Useful commands:"
echo "  systemctl status rsplayer"
echo "  journalctl -u rsplayer -f -n 50"
echo "========================================"
