![](https://github.com/ljufa/rsplayer/actions/workflows/ci.yml/badge.svg)
![](https://github.com/ljufa/rsplayer/actions/workflows/cd.yml/badge.svg)
![](https://github.com/ljufa/rsplayer/actions/workflows/docker.yml/badge.svg)
![](https://img.shields.io/github/v/release/ljufa/rsplayer)
![](https://img.shields.io/github/license/ljufa/rsplayer?style=flat-square)
![](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=flat-square)
# RSPlayer
RSPlayer is open-source music player designed specifically for headless computing environments. It shines on devices like the Raspberry Pi and other Linux-powered Single Board Computers (SBCs).

Operating as a system service, RSPlayer offers a web-based user interface, making it a perfect fit for devices without dedicated monitors or input peripherals. The UI is meticulously designed to be responsive and intuitive, delivering a seamless user experience across mobile devices, tablets, and PCs.

Under the hood, RSPlayer harnesses the power of the [Symphonia](https://github.com/pdeljanov/Symphonia) and [Cpal](https://github.com/rustaudio/cpal) crates. These allow RSPlayer to handle audio decoding and playback efficiently, leveraging Rust's native capabilities for high-performance audio playback.

For DIY enthusiasts seeking a customizable, high-performance music player for their projects, RSPlayer is the go-to choice. Its lightweight design and efficient resource usage make it ideal for transforming your Raspberry Pi or other SBCs into a dedicated music station.

### Online demo -> https://rsplayer.dlj.freemyip.com/

## Features
- **Low Latency Output**: Direct output to ALSA minimizes latency.
- **Adjustable Playback Thread Priority**: Customize the priority of the playback thread up to a real-time rating of 99 via the settings page.
- **Dedicated CPU Core for Playback**: By default, the playback thread is pinned to a single CPU core for optimized performance.
- **Web UI Remote Control**: Manage your playback remotely with an intuitive web interface.
- **Flexible Volume Control**: Control the volume using software (alsa mixer) or hardware control (Dac chip instuctions via GPIO).
- **Written in Rust**: Enjoy the benefits of minimal dependencies and high performance, thanks to the Rust native implementation.
- **Comprehensive Music Library Management**: Scan, search, and browse your music library and online radio stations with ease.
- **Dynamic Playlists**: Automaticaly create dynamic playlists for personalized listening experiences.
- **DSP Integration**: Advanced Digital Signal Processing with filters and presets.
- **Web UI VU Meter**: Real-time audio visualization in the web interface.
- **Extended Hardware Control**: Support for seek and power management via firmware interactions.

### Planed features
- **Expanded Audio Codec Support**: Compatibility with a wider range of audio codecs.
- **Intelligent Dynamic Playlists**: Advanced dynamic playlists that adapt based on user likes or playback counts for a personalized listening experience.
- **Windows Compatibility**: Development of a Windows build to extend platform support.
- **MacOS Compatibility**: Development of a MacOS build to extend platform support.
- **Remote file system management**: Ability to mount and use remote file storage (nfs and samba) from UI. 
- **Lyrics Support**: Display synchronized or unsynchronized lyrics.
- **Music Recommendations**: Suggest similar tracks or artists based on listening history or current playback.
- **Web UI Themes**: Support for customizable themes and dark/light modes.

## Installation 
To install RSPlayer on debian based linux distro, execute the following script (requires curl):
```bash
bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
```
The installation script will install all the necessary files, configure and start the systemd service.

To stop RSPlayer, run the following command:
```bash
sudo systemctl stop rsplayer
```
To start RSPlayer service again, run the following command:
```bash
sudo systemctl start rsplayer
```
## Run as docker container
```bash
docker run -p 8000:80 -v ${MUSIC_DIR}:/music -v rsplayer_data:/opt/rsplayer --device /dev/snd -it --rm ljufa/rsplayer:latest
```
or [docker compose](docker-compose.yaml)
```yaml
services:
  rsplayer:
    image: ljufa/rsplayer:latest
    devices:
      - /dev/snd
    ports:
      - 8000:80
    volumes:
      - ${MUSIC_DIR}:/music:ro
      - 'rsplayer_volume:/opt/rsplayer'
    restart: unless-stopped
volumes:
  rsplayer_volume:
    driver: local
```

## Usage
Once RSPlayer is installed, you can access the web user interface by navigating to http://localhost or the IP address of the machine on which it is installed.

For detailed configuration instructions, please refer to the [documentation](https://ljufa.github.io/rsplayer/#/?id=basic-configuration).

## Home Assistant Integration
RSPlayer can be controlled from [Home Assistant](https://www.home-assistant.io/) via the [rsplayer_hacs_plugin](https://github.com/ljufa/rsplayer_hacs_plugin).

Features include media player control (play, pause, stop, next/prev, volume) and real-time sync with `rsplayer_firmware` power state.

Install via HACS by adding `https://github.com/ljufa/rsplayer_hacs_plugin` as a custom repository.

## DIY Hardware
For DIY enthusiasts, `rsplayer` offers the flexibility to integrate with custom hardware components.

- **Hardware Designs**: [rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware)
- **Firmware**: [rsplayer_firmware](https://github.com/ljufa/rsplayer_firmware)

See the [Hardware Integration documentation](https://ljufa.github.io/rsplayer/#/?id=hardware-integration) for more details.

## Contributing
If you would like to contribute to RSPlayer, please submit a pull request or open an issue on the GitHub repository.

For instructions on how to build the project from source, please see the [BUILD.md](BUILD.md) file.

## License
RSPlayer is licensed under the MIT license. See the LICENSE file for more information.
