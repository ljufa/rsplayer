#!/usr/bin/env bash
set -e
device_arch=`arch`
# device_arch="armv7l"
# device_arch="armv7l"
# device_arch="x86_64"

echo "Device architecture is:"${device_arch}
arch_expr="unknown"

if [ "$device_arch" = "aarch64" ]; then
    arch_expr="_arm64"
elif [ "$device_arch" = "x86_64" ]; then
    arch_expr="_amd64"
elif [ "$device_arch" = "armv7l" ]; then
    arch_expr="_armhf"
else
    arch_expr="unknown_architecture"
fi

echo "Detected architecture suffix is:"${arch_expr}
sudo systemctl stop rsplayer || true
URL=`curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | grep ${arch_expr}.deb | cut -d '"' -f 4`
echo Downloading installation package from $URL ...
curl -L -o rsplayer${arch_expr}.deb  $URL
sudo dpkg -i --force-overwrite rsplayer${arch_expr}.deb
sudo systemctl daemon-reload
sudo systemctl enable rsplayer
rm rsplayer${arch_expr}.deb
sleep 2
sudo systemctl start rsplayer
echo Done!
