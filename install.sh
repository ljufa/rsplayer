#!/usr/bin/env bash
set -e
device_arch=`arch`
echo ${device_arch}
arch_expr="unknown"
arch_aarch64="aarch64"
arch_armhf="armv7l"
if [ "$device_arch" = "$arch_aarch64" ]; then
    arch_expr="_arm64"
else
    arch_expr="_armhf"
fi
echo ${arch_expr}
URL=`curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | grep ${arch_expr} | cut -d '"' -f 4`
echo Downloading installation package from $URL ...
wget -O rsplayer${arch_expr}.deb  $URL
sudo dpkg -i --force-overwrite rsplayer${arch_expr}.deb
sudo systemctl enable rsplayer
rm rsplayer${arch_expr}.deb
echo Done! Please reboot.