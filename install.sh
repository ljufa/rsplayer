#!/usr/bin/env bash
set -e
device_arch=$(arch)
if [ "$device_arch" = "x86_64" ]; then
    device_arch="amd64"
elif [ "$device_arch" = "aarch64" ]; then
    device_arch="arm64"
elif [ "$device_arch" = "armv7l" ] || [ "$device_arch" = "armv6l" ]; then
    device_arch="armhf"
fi
echo "Using architecture:${device_arch}"
deb_file_name="rsplayer_${device_arch}"
URL=$(curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | cut -d '"' -f 4 | grep "_${device_arch}.deb" | head -n 1)
echo Downloading installation package from "$URL" ...
curl -L -o "${deb_file_name}".deb  "$URL"
sudo dpkg -i --force-overwrite "${deb_file_name}".deb
rm "${deb_file_name}".deb
