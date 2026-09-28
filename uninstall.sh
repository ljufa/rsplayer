#!/usr/bin/env bash
set -e

# Stop and disable service
sudo systemctl stop rsplayer || true
sudo systemctl disable rsplayer || true

device_arch=$(uname -m)

# Map device architecture to package suffix per distribution
if [ "$device_arch" = "x86_64" ]; then
    deb_arch_suffix="amd64"
    rpm_arch_suffix="x86_64"
    arch_arch_suffix="x86_64"
elif [ "$device_arch" = "aarch64" ]; then
    deb_arch_suffix="arm64"
    rpm_arch_suffix="aarch64"
    arch_arch_suffix="aarch64"
elif [ "$device_arch" = "armv7l" ]; then
    deb_arch_suffix="armhfv7"
    rpm_arch_suffix="armv7hl"
    arch_arch_suffix="armv7h"
elif [ "$device_arch" = "armv6l" ]; then
    deb_arch_suffix="armhfv6"
    rpm_arch_suffix="armv6hl"
    arch_arch_suffix="armv6h"
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
            pkg_ext="pkg.tar.zst"
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

case $pkg_type in
    deb)
        # Remove DEB package
        sudo dpkg -r rsplayer || true
        ;;
    rpm)
        # Remove RPM package
        sudo rpm -e rsplayer || true
        ;;
    arch)
        if pacman -Q rsplayer-bin >/dev/null 2>&1; then
            sudo pacman -R rsplayer-bin
        else
            # Tarball installed by an earlier installer version
            sudo rm -f /usr/bin/rsplayer
            sudo rm -f /etc/systemd/system/rsplayer.service
            sudo rm -f /etc/polkit-1/rules.d/99-rsplayer.rules
            sudo rm -rf /opt/rsplayer
            echo "Arch tarball installation removed."
        fi
        ;;
esac

# Reload systemd
sudo systemctl daemon-reload || true

echo "Uninstallation complete. Note: User 'rsplayer' and group 'rsplayer' were not removed automatically."
echo "To remove them manually, run:"
echo "  sudo userdel rsplayer"
echo "  sudo groupdel rsplayer"