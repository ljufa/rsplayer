# Install
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

## Basic installation
### Install or upgrade
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

### Verify installation
* Run systemd service by `sudo systemctl start rsplayer`
* Check service status by `sudo systemctl status rsplayer` and if it shows active go to the next step
* Open browser at http://you-machine-ip-address i.e. http://raspberrypi.local. 

?>TIP: The HTTP and HTTPS ports are configured in the `/opt/rsplayer/env` file. By default, `PORT` is set to 80 and `TLS_PORT` is set to 443. You can edit this file to change the ports used by `rsplayer`.
* If the page can not load or there is an error message at top of the page please see the [Troubleshooting](?id=troubleshooting) section.

# Basic Configuration

To configure `rsplayer`, navigate to the settings page in the web UI. Here is an overview of the available settings.

## General

-   **Audio interface:** Selects the primary audio device for playback. Options include your available ALSA hardware cards, a `Pipewire` virtual card (if `wpctl` is installed on the host), or `Local Browser Playback` for streaming audio directly to your device's web browser.
-   **PCM Device:** Choose the specific PCM device for the selected audio interface (hidden if Local Browser Playback is selected).
-   **Input buffer size (MB):** The size of the buffer for audio data, in megabytes (1-200). (Hidden if Local Browser Playback is selected).
-   **Ring buffer size (ms):** The size of the ring buffer in milliseconds (1-10000). (Hidden if Local Browser Playback is selected).
-   **Player thread priority:** The priority of the player thread, from 1 to 99. (Hidden if Local Browser Playback is selected).
-   **Set alsa buffer frame size (Experimental!):** An experimental feature to set the ALSA buffer frame size. (Hidden if Local Browser Playback is selected).
-   **Music directory path:** The full path to your music library. After changing this, you need to click "Update library" or "Full rescan".
-   **Auto resume playback on start:** If enabled, `rsplayer` will automatically resume playback of the last track when it starts.

## Volume control

-   **Volume control device:** Select the method for controlling volume (e.g., Alsa, Pipewire).
-   **Alsa mixer:** If "Alsa" is chosen, this selects the specific mixer control.
-   **Volume step:** The amount to increase or decrease the volume with each step.

## RSPlayer firmware(control board) USB link

-   **Enable link with rsplayer firmware:** Enables communication over a USB serial connection with custom rsplayer firmware control boards.
-   **(When enabled):** Provides quick action buttons to **Power Off** or **Power On** the connected firmware hardware.

# Hardware Integration

For DIY enthusiasts looking to integrate `rsplayer` with custom hardware, all related resources have been moved to dedicated repositories to streamline development and maintenance.

- **Hardware Designs & KiCad Files:** All hardware schematics, PCB layouts (KiCad), and documentation are available in the [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware) repository.
- **Firmware:** The firmware for microcontrollers and other hardware components is located in the [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware) repository.

Please refer to the documentation within these repositories for detailed guides on hardware setup, configuration, and development.

# Usage
## Player
TODO
## Queue page
TODO
## Playlist page
 TODO

-------

# Troubleshooting
?>If you can't access http://rsplayer.local from your android phone use RPI ip address or PC browser. At the time mDns/zeroconf is not supported by Android.

## Useful commands
* get logs 
```bash
journalctl -u rsplayer.service -f -n 300
```
* restart rsplayer 
```bash
sudo systemctl restart rsplayer
```
For configuration related troubleshooting you can find configuration file at `/opt/rsplayer/configuration.yaml`

## Music library scan finds no files from a mounted drive (Samba/NFS)

If the library scan completes immediately with no files found, it is most likely a file permission issue. The `rsplayer` service runs as the `rsplayer` user, which may not have read access to your mount point.

**Verify the issue:**
```bash
sudo -u rsplayer ls /mnt/your-mount-path
```
If this returns "Permission denied", the `rsplayer` user cannot access the mount.

**Fix for Samba (CIFS) mounts:**

Edit your `/etc/fstab` entry and add `uid` and `gid` options so the `rsplayer` user owns the mounted files:
```
//server/share /mnt/samba/music cifs credentials=/etc/samba/creds,uid=rsplayer,gid=rsplayer,file_mode=0644,dir_mode=0755 0 0
```
Then remount and restart:
```bash
sudo mount -a
sudo systemctl restart rsplayer
```

**Fix for NFS mounts:**

Use `all_squash` with `anonuid`/`anongid` on the NFS server side, or add the `rsplayer` user to a group that has read access:
```bash
sudo usermod -aG <mount-group> rsplayer
sudo systemctl restart rsplayer
```

**Also check:**
- The **Music directory path** in RSPlayer settings matches the exact mount path where your audio files are located.
- The `cifs-utils` package is installed if using Samba (`sudo apt install cifs-utils`).

## RSPlayer fails to start because the port is already in use

If `rsplayer` fails to start and the logs show an "address already in use" error, another service (e.g., Apache, Nginx, or another web server) is already using the configured port.

**Check what is using the port:**
```bash
sudo ss -tlnp | grep ':80'
```

**Fix — change the RSPlayer port:**

Edit the environment file `/opt/rsplayer/env` and set `PORT` and/or `TLS_PORT` to available ports:
```
PORT=8080
TLS_PORT=8443
```

Then restart the service:
```bash
sudo systemctl restart rsplayer
```

You will now access the web UI at `http://your-machine-ip:8080` instead of the default `http://your-machine-ip`.

**Alternative — stop the conflicting service:**
```bash
sudo systemctl stop <conflicting-service>
sudo systemctl disable <conflicting-service>
sudo systemctl restart rsplayer
```

## Audio playback stutters or breaks on low-spec hardware

On lower-powered devices like the Raspberry Pi Zero, Pi 2, or Pi 3A+, playback may stutter, skip, or stop entirely due to the default ALSA buffer size being too small for the hardware to keep up. You can confirm this issue by checking the logs — look for ALSA poll errors:
```bash
journalctl -u rsplayer.service -f -n 300
```

**Fix — increase the ALSA buffer frame size:**

1. Open the RSPlayer settings page in your browser.
2. Enable **Set alsa buffer frame size (Experimental!)**.
3. Start with a value of **2000** and test playback.
4. If playback still breaks, increase gradually: **3000**, **4000**, **5000**, etc., until playback is stable.
5. Use the lowest value that gives stable playback, as larger buffers add more latency.

**Additional tips for low-spec hardware:**
- Increase the **Ring buffer size (ms)** — try values like 2000–5000 ms.
- Lower the **Player thread priority** if you notice the system becoming unresponsive.
- Avoid high-resolution files (e.g., 24-bit/192kHz) if your device struggles — standard 16-bit/44.1kHz is much less demanding.

## Playlist page is empty
TODO

### TODO...

-------
 