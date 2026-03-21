# Installation

## Supported hardware and OS
RSPlayer can be installed on Linux systems with the following CPU architectures:
* [x] Linux amd64(x86_64-unknown-linux-gnu) - x86 Intel and AMD CPUs
* [x] Linux aarch64(aarch64-unknown-linux-gnu) - ARM 64-bit CPUs: RPI 5, RPI 4 and other ARMv8 based boards
* [x] Linux armv7(armv7-unknown-linux-gnueabihf) - ARM 32-bit CPUs: RPI 4 (32-bit), RPI 3, RPI 2
* [x] Linux armv6(arm-unknown-linux-gnueabihf) - ARM 32-bit CPUs: RPI Zero, RPI Zero W, RPI 1
* [ ] Android
* [ ] Windows x86_64
* [ ] Windows aarch64
* [ ] MacOS
* [ ] FreeBSD

## Install or upgrade
RSPlayer can be installed using one of two methods:
* Using installation script (automatically detects your distribution and architecture)
```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
```
The installation script detects your Linux distribution (Debian/Ubuntu, Fedora/RHEL/CentOS, Arch/Manjaro) and installs the appropriate package type (.deb, .rpm, or .tgz tarball).

* Manually download and install package
The latest packages can be downloaded from [this page](https://github.com/ljufa/rsplayer/releases/latest). Available package types:
- **DEB packages**: For Debian, Ubuntu, Raspbian (`rsplayer_*_amd64.deb`, `rsplayer_*_arm64.deb`, `rsplayer_*_armhfv7.deb`, `rsplayer_*_armhfv6.deb`)
- **RPM packages**: For Fedora, RHEL, CentOS, openSUSE (`rsplayer_*_x86_64.rpm`, `rsplayer_*_aarch64.rpm`, `rsplayer_*_armv7hl.rpm`, `rsplayer_*_armv6hl.rpm`)
- **Arch tarballs**: For Arch Linux, Manjaro (`rsplayer_*_amd64.tgz`, `rsplayer_*_arm64.tgz`, `rsplayer_*_armhfv7.tgz`, `rsplayer_*_armhfv6.tgz`)

* Download and manually install binary file
  - Under latest release page find `rsplayer_*` file for your system and download
  - rename file to `rsplayer`
  - make it executable using `chmod +x rsplayer`
  - run using command `./rsplayer`
  - optionally if you need to run rsplayer automatically as a service use [this systemd service file](../PKGS/debian/etc/systemd/system/rsplayer.service)

## Verify installation
* Run systemd service by `sudo systemctl start rsplayer`
* Check service status by `sudo systemctl status rsplayer` and if it shows active go to the next step
* Open browser at http://you-machine-ip-address i.e. http://raspberrypi.local.

?>TIP: The HTTP and HTTPS ports are configured in the `/opt/rsplayer/env` file. By default, `PORT` is set to 80 and `TLS_PORT` is set to 443. You can edit this file to change the ports used by `rsplayer`.
* If the page can not load or there is an error message at top of the page please see the [Troubleshooting](troubleshooting.md) section.
