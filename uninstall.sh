#!/usr/bin/env bash
sudo systemctl stop rsplayer
sudo systemctl disable rsplayer
device_arch=$(arch)
echo "Device architecture is:${device_arch}"
deb_file_name="rsplayer_${device_arch}"
sudo dpkg -r $deb_file_name
# sudo rm -r /opt/rsplayer
sudo userdel -r rsplayer
sudo systemctl daemon-reload