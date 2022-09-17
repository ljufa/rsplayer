#!/usr/bin/env bash
URL=`curl -s https://api.github.com/repos/ljufa/rsplayer/releases/latest | grep browser_download_url | grep arm64 | cut -d '"' -f 4`
echo Downloading installation package from $URL ...
wget -O rsplayer_latest_arm64.deb $URL
sudo dpkg -i --force-overwrite rsplayer_latest_arm64.deb
sudo systemctl enable rsplayer
rm rsplayer_latest_arm64.deb
echo Done! Please reboot.