#!/usr/bin/env bash
set -e
device_arch=$(arch)

# Map device architecture to package suffix per distribution
# Debian/Ubuntu suffixes
if [ "$device_arch" = "x86_64" ]; then
    deb_arch_suffix="amd64"
    rpm_arch_suffix="x86_64"
    arch_arch_suffix="x86_64"
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

# Detect distribution
if [ -f /etc/os-release ]; then
    . /etc/os-release
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
            # Default to deb
            pkg_type="deb"
            pkg_suffix="$deb_arch_suffix"
            pkg_ext="deb"
            ;;
    esac
else
    # Assume deb
    pkg_type="deb"
    pkg_suffix="$deb_arch_suffix"
    pkg_ext="deb"
fi

echo "Detected $pkg_type package for architecture $pkg_suffix"

# Function to attempt download
try_download() {
    local type=$1
    local suffix=$2
    local ext=$3
    local file="rsplayer_${suffix}.${ext}"
    
    echo "Attempting to download $type package for $suffix..."
    URL=$(curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | cut -d '"' -f 4 | grep "_${suffix}.${ext}" | head -n 1)
    if [ -z "$URL" ]; then
        echo "No $type package found for architecture $suffix"
        return 1
    fi
    echo "Downloading $type package from $URL..."
    if curl -L -o "$file" "$URL"; then
        echo "Download successful"
        pkg_type="$type"
        pkg_suffix="$suffix"
        pkg_ext="$ext"
        pkg_file_name="$file"
        return 0
    else
        echo "Download failed"
        rm -f "$file"
        return 1
    fi
}

# Try primary package type
if ! try_download "$pkg_type" "$pkg_suffix" "$pkg_ext"; then
    echo "Primary package type $pkg_type not available, falling back to DEB"
    # Fall back to DEB
    if ! try_download "deb" "$deb_arch_suffix" "deb"; then
        echo "ERROR: No suitable package found for architecture $device_arch"
        exit 1
    fi
fi

case $pkg_type in
    deb)
        sudo dpkg -i --force-overwrite "${pkg_file_name}"
        ;;
    rpm)
        sudo rpm -i --force "${pkg_file_name}"
        ;;
    arch)
        # Extract tarball to root
        sudo tar -xzf "${pkg_file_name}" -C /
        # Run post-install steps
        # Add user and groups if not exist
        getent group audio >/dev/null || sudo groupadd -r audio
        getent group dialout >/dev/null || sudo groupadd -r dialout
        getent group i2c >/dev/null || sudo groupadd -r i2c
        getent group gpio >/dev/null || sudo groupadd -r gpio
        getent group spi >/dev/null || sudo groupadd -r spi
        getent group input >/dev/null || sudo groupadd -r input
        getent group rsplayer >/dev/null || sudo groupadd -r rsplayer
        if ! getent passwd rsplayer >/dev/null; then
            sudo useradd -r -s /bin/false -d /opt/rsplayer -g rsplayer rsplayer || :
            sudo usermod -a -G audio,dialout,i2c,gpio,spi,input rsplayer || true
        fi
        sudo chown -R rsplayer:rsplayer /opt/rsplayer
        sudo systemctl daemon-reload
        sudo systemctl enable rsplayer
        sudo systemctl start rsplayer
        ;;
esac

rm "${pkg_file_name}"
