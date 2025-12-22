# Install
## Supported hardware and OS
RSPlayer can be installed on Linux systems with the following CPU architectures:
* [x] Linux amd64(x86_64-unknown-linux-gnu) - x86 intel and amd cpus 
* [x] Linux aarch64(aarch64-unknown-linux-gnu) - arm 64bit cpus: RPI4 and other arm8 cpu based boards ...
* [x] Linux armv7(armv7-unknown-linux-gnueabihf) - arm 32bit cpus: RP4(32bit), RPI3, RPI2, RPI zero ...
* [ ] Android
* [ ] Windows x84_64
* [ ] Windows aarch64
* [ ] MacOS
* [ ] FreeBSD

## Basic installation
### Install or upgrade
RSPlayer can be installed using one of two methods:
* Using installation script(it will detect your architecture)
```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
```
* Manually download and install deb package
The latest package can be downloaded from [this page](https://github.com/ljufa/rsplayer/releases/latest).

* Download and manually install binary file (for non debian based linux)
  - Under latest release page find `rsplayer_*` file for your system amd download
  - rename file to `rsplayer`
  - make it executable using `chmod +x rsplayer`
  - run using command `./rsplayer`
  - optionally if you need to run rsplayer automatically as as a service use [this systemd service file](../PKGS/debian/etc/systemd/system/rsplayer.service)

### Verify installation
* Run systemd service by `sudo systemctl start rsplayer`
* Check service status by `sudo systemctl status rsplayer` and if it shows active go to the next step
* Open browser at http://you-machine-ip-address i.e. http://raspberrypi.local. 

?>TIP: The HTTP and HTTPS ports are configured in the `/opt/rsplayer/env` file. By default, `PORT` is set to 80 and `TLS_PORT` is set to 443. You can edit this file to change the ports used by `rsplayer`.
* If the page can not load or there is an error message at top of the page please see the [Troubleshooting](?id=troubleshooting) section.

# Basic Configuration

To configure `rsplayer`, navigate to the settings page in the web UI. Here is an overview of the available settings.

## General

-   **Audio interface:** Selects the primary ALSA audio device for playback.
-   **PCM Device:** Choose the specific PCM device for the selected audio interface.
-   **Input buffer size (MB):** The size of the buffer for audio data, in megabytes (1-200).
-   **Ring buffer size (ms):** The size of the ring buffer in milliseconds (1-10000).
-   **Player thread priority:** The priority of the player thread, from 1 to 99.
-   **Set alsa buffer frame size (Experimental!):** An experimental feature to set the ALSA buffer frame size.
-   **Music directory path:** The full path to your music library. After changing this, you need to click "Update library" or "Full rescan".
-   **Auto resume playback on start:** If enabled, `rsplayer` will automatically resume playback of the last track when it starts.

## Volume control

-   **Volume control device:** Select the method for controlling volume (e.g., Alsa).
-   **Alsa mixer:** If "Alsa" is chosen, this selects the specific mixer control.
-   **Volume step:** The amount to increase or decrease the volume with each step.

## USB

-   **Enable USB channel communication?:** Enables communication over a USB serial connection.
-   **Serial device:** The device path for the USB serial connection (e.g., `/dev/ttyAMA0`).

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

## RSPlayer server can't start
TODO

## Playlist page is empty
TODO

### TODO...

-------

# Roadmap
 
## Features
* [x] Make artist/album/song as a link on playback page
* [x] Replace UART communication with rsplayer_firmware with USB
* [ ] Implement MQTT or other homeassistant friendly communication channel 
* [x] Show files as a tree
* [x] Show media library (artist/album) as a tree
* [x] Web radio browse/play
* [x] Like/Favorite song/radio
* [x] Search files/radio/artist
* [ ] Playlist by genre
* [ ] Most played songs playlist
* [ ] Liked songs playlist
* [x] Show favorited radios on top of the list 
* [ ] New music mix dynamic playlist
* [ ] Generate missing album cover image using album name
* [x] Seek to position
* [x] Keep last N songs in *history* when random mode is enabled
* [ ] Windows support
* [ ] MacOS support
* [x] DSD/DoP playback support
* [ ] use more information about the song based on last.fm response, update id tags on local files?
* [ ] streaming to local device (i.e. phone) for i.e. preview
* [ ] Add all settings to settings page
## Code improvements
* [ ] replace sled with native_db or redb (high memory usage)
* [ ] get rid of `.unwrap()` calls
* [ ] get rid of unnecessary `.clone()` calls
* [ ] refactor names all over the code
* [ ] replace `warp` with `axum` or `actix`
* [ ] write generic fun and macros to reduce code duplication in UI

-------
 