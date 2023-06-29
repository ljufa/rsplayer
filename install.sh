#!/usr/bin/env bash
set -e
device_arch=$(arch)
echo "Device architecture is:${device_arch}"

deb_file_name="rsplayer_${device_arch}"
URL=$(curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | grep ${deb_file_name} | grep .deb | cut -d '"' -f 4)
echo Downloading installation package from "$URL" ...
curl -L -o ${deb_file_name}.deb  "$URL"
sudo dpkg -i --force-overwrite ${deb_file_name}.deb
rm ${deb_file_name}.deb
