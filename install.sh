#!/usr/bin/env bash
set -e
device_arch=$(arch)
deb_arch_suffix=""

if [ "$device_arch" = "x86_64" ]; then
    deb_arch_suffix="amd64"
elif [ "$device_arch" = "aarch64" ]; then
    deb_arch_suffix="arm64"
elif [ "$device_arch" = "armv7l" ]; then
    deb_arch_suffix="armhfv7"
elif [ "$device_arch" = "armv6l" ]; then
    deb_arch_suffix="armhfv6"
else
    deb_arch_suffix=$device_arch
fi

echo "Using architecture suffix:${deb_arch_suffix}"
deb_file_name="rsplayer_${deb_arch_suffix}"
URL=$(curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | cut -d '"' -f 4 | grep "_${deb_arch_suffix}.deb" | head -n 1)
echo Downloading installation package from "$URL" ...
curl -L -o "${deb_file_name}".deb  "$URL"
sudo dpkg -i --force-overwrite "${deb_file_name}".deb
rm "${deb_file_name}".deb
