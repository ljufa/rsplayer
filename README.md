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
- **Infrared Remote Control**: Use LIRC for convenient control with an infrared remote.
- **Written in Rust**: Enjoy the benefits of minimal dependencies and high performance, thanks to the Rust native implementation.
- **Comprehensive Music Library Management**: Scan, search, and browse your music library and online radio stations with ease.
- **Dynamic Playlists**: Automaticaly create dynamic playlists for personalized listening experiences.

### Planed features

- **DSD and DoP Playback**: Implement support for Direct Stream Digital (DSD) and Digital over PCM (DoP) playback for high-quality audio.
- **Expanded Audio Codec Support**: Compatibility with a wider range of audio codecs.
- **Intelligent Dynamic Playlists**: Advanced dynamic playlists that adapt based on user likes or playback counts for a personalized listening experience.
- **Windows Compatibility**: Development of a Windows build to extend platform support.
- **MacOS Compatibility**: Development of a MacOS build to extend platform support.
- **DSP Support**: Integration of Digital Signal Processing (DSP) capabilities for enhanced audio effects and manipulations.
- **Remote file system management**: Ability to mount and use remote file storage (nfs and samba) from UI. 
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
version: "3"
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
Once RSPlayer is installed, you can access the web user interface by navigating to http://localhost or the IP address of the machine on which it is installed. From the web user interface, you can finish configuration following steps described [here](https://ljufa.github.io/rsplayer/#/?id=basic-configuration).

For minimal working configuration it is required to select *Audio interface*, *PCM device*, *Music directory path* followed by *Update library*.
#### Detailed documentation -> https://ljufa.github.io/rsplayer/

 ## Requirements
* x86(Amd64) or Arm(64 and 32bit) computer with debian based linux distribution.

## Tested on
* Rpi4 - RpiOS bookworm and bullseye
* Rpi Zero WH - RpiOS bookworm and bullseye
* Various x86_64 laptop/pc with ubuntu and debian based distros

## With additional hardware devices it provides additional features
* Hardware volume control by DAC chip
* Infrared remote control: Play, Pause, Next, Prev, Volume Up/Down, Poweroff
* Volume control using rotary encoder
* Oled display for song and player info
* Switch audio output between speakers and headphones
* Change DAC settings: digital filter, gain, sound profile

#### Example hardware list for DIY streamer implementation:
* Diy friendly AK44xx DAC board i.e. [Diyinhk](https://www.diyinhk.com/shop/audio-kits/), [JLSounds](http://jlsounds.com/products.html) ...
* USB to I2S converter board. i.e. [WaveIO](https://luckit.biz/), [Amanero](https://amanero.com/), [JLSounds](http://jlsounds.com/products.html) ...
* Infrared Receiver TSOP312xx. i.e. [TSOP31238](https://eu.mouser.com/ProductDetail/Vishay-Semiconductors/TSOP31238?qs=5rGgbCH0pB1jaK4I0GvRsw%3D%3D)
* A1156 Apple Remote Control
* Oled display ST7920 128x64 (from Amazon, Ebay ...)
* Rotary Encoder (from Amazon, Ebay ...)
* Headphone Amp board i.e. [Whammy](https://diyaudiostore.com/products/whammy-completion-kit?_pos=3&_sid=bf6542f23&_ss=r)
* Power Supply
* Metal Case

## Contributing
If you would like to contribute to RSPlayer, please submit a pull request or open an issue on the GitHub repository.

## License
RSPlayer is licensed under the MIT license. See the LICENSE file for more information.
