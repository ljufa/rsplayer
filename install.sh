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

REPO_URL="https://ljufa.github.io/rsplayer-pkg"

# Add the RSPlayer apt repo and install from it. Every step is chained so any
# failure returns non-zero and the caller falls back to a direct download.
setup_apt_repo() {
    echo "[INFO] Adding RSPlayer apt repository ($REPO_URL/deb)"
    curl -fsSL "$REPO_URL/rsplayer.gpg" -o /tmp/rsplayer-keyring.gpg &&
        $SUDO install -D -m 644 /tmp/rsplayer-keyring.gpg /usr/share/keyrings/rsplayer.gpg &&
        rm -f /tmp/rsplayer-keyring.gpg &&
        echo "deb [signed-by=/usr/share/keyrings/rsplayer.gpg] $REPO_URL/deb stable main" | $SUDO tee /etc/apt/sources.list.d/rsplayer.list >/dev/null &&
        $SUDO apt-get update &&
        $SUDO apt-get install -y rsplayer
}

setup_dnf_repo() {
    echo "[INFO] Adding RSPlayer dnf repository ($REPO_URL/rpm)"
    curl -fsSL "$REPO_URL/rpm/rsplayer.repo" -o /tmp/rsplayer.repo &&
        $SUDO install -m 644 /tmp/rsplayer.repo /etc/yum.repos.d/rsplayer.repo &&
        rm -f /tmp/rsplayer.repo &&
        $SUDO dnf install -y rsplayer
}

# Prefer the package repository: after the one-time setup, updates arrive via
# regular apt/dnf upgrade. Pre-release installs and Arch/unknown distros use
# the direct-download path below (also the fallback if the repo fails).
repo_install_done=false
if [ "$PRE_RELEASE" = false ]; then
    case $pkg_type in
        deb)
            if setup_apt_repo; then
                repo_install_done=true
            else
                echo "[WARN] Repo-based install failed, removing repo entry and falling back to direct package download"
                $SUDO rm -f /etc/apt/sources.list.d/rsplayer.list
            fi
            ;;
        rpm)
            if setup_dnf_repo; then
                repo_install_done=true
            else
                echo "[WARN] Repo-based install failed, removing repo entry and falling back to direct package download"
                $SUDO rm -f /etc/yum.repos.d/rsplayer.repo
            fi
            ;;
    esac
fi

# Function to attempt download
try_download() {
    local type=$1
    local suffix=$2
    local ext=$3
    local file="rsplayer_${suffix}.${ext}"

    if [ "$PRE_RELEASE" = true ]; then
        echo "[INFO] Querying GitHub API for latest pre-release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases?per_page=1" | grep browser_download_url | cut -d '"' -f 4 | grep "_${suffix}.${ext}" | grep -v "/rsplayer-desktop_" | head -n 1)
    else
        echo "[INFO] Querying GitHub API for latest stable release..."
        URL=$(curl -s "https://api.github.com/repos/ljufa/rsplayer/releases/latest" | grep browser_download_url | cut -d '"' -f 4 | grep "_${suffix}.${ext}" | grep -v "/rsplayer-desktop_" | head -n 1)
    fi
    if [ -z "$URL" ]; then
        echo "[WARN] No $type package found for suffix=$suffix ext=$ext"
        return 1
    fi
    echo "[INFO] Downloading: $URL"
    if curl -L -o "$file" "$URL"; then
        echo "[INFO] Download OK: $file ($(du -h "$file" | cut -f1))"
        chmod 644 "$file"
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

# Direct-download install (pre-releases, Arch, unknown distros, repo fallback)
if [ "$repo_install_done" = false ]; then

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
        echo "[INFO] Installing alsa-lib dependency..."
        if pacman -Q alsa-lib >/dev/null 2>&1; then
            echo "[INFO] alsa-lib already installed, skipping"
        elif ! $SUDO pacman -S --needed --noconfirm alsa-lib; then
            echo "[WARN] Failed to install alsa-lib via pacman (possibly a stale multilib sync db)."
            echo "[WARN] Try 'sudo pacman -Syu' to fully sync/upgrade your system, then re-run this installer."
            echo "[WARN] Continuing installation; rsplayer may fail to start without alsa-lib."
        fi
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

fi # repo_install_done

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
