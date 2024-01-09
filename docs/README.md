# Install
## Supported hardware and OS
RSPlayer can be installed on Linux systems with the following CPU architectures:
* [x] Linux amd64(x86_64-unknown-linux-gnu) - x86 intel and amd cpus 
* [x] Linux aarch64(aarch64-unknown-linux-gnu) - arm 64bit cpus: RPI4 and other arm8 cpu based boards ...
* [x] Linux armv7(armv7-unknown-linux-gnueabihf) - arm 32bit cpus: RP4(32bit), RPI3, RPI2, RPI zero ...
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

?>TIP: If port 80 is not available it will automatically fall back to port 8000 so in this case UI will be available at http://raspberrypi.local:8000. Custom port can be specified by editing `/etc/systemd/system/rsplayer.service` file
* If the page can not load or there is an error message at top of the page please see the [Troubleshooting](?id=troubleshooting) section.


# Basic configuration
## Players
By default, RSPlayer is configured to use its own media player.
To make configuration changes navigate to [http://rsplayer.local/#settings](http://rsplayer.local/#settings).

### RSP
RSPlayer playback implementation based on rust Symphonia crate.
* _Input buffer size_ - Size of input audio buffer

## Audio interface
This is an alsa audio interface that will be used by active player
## PCM Device
Alsa PCM output device of selected audio interface

## Music directory path
Full path to music root music directory, will be used by RSP.
Please keep in mind that after this value is changed or set for the first time `Update library` button should be clicked and the music database created.

## Volume control
* _Volume control device_ - Select volume control device: Dac or Alsa
* _Alsa mixer_ - If Alsa is volume control device then alsa mixer should be selected here
* _Volume step_ - How many units to send to the control device for a single button press or encoder step
* _Enable rotary encoder_ - Enable if you use a rotary encoder

# Advanced configuration for additional hardware support (RPI only)

## Install RPI OS
If you are going to install RPI os from scratch it is important to enable ssh, and wifi and specify the hostname.

  * For os image select _Raspberry PI OS Lite 64-bit_
  * Click on the gear icon and enable the following options

    ![](_assets/pi_imager_options.png ':size=450')

## Configure Raspberry PI
After installation is done ssh login to RPI `ssh pi@rsplayer.local` and make the following changes:
* Enable SPI and I2C options using `raspi-config` tool
* Make sure you have the following entries in `/boot/config.txt`:
    ```json
    dtoverlay=gpio-ir,gpio_pin=17
    dtoverlay=rotary-encoder,pin_a=15,pin_b=18,relative_axis=1,steps-per-period=1
    gpio=18,15,19=pu
    gpio=22,23=op,dh
    ```

 
-------
## External hardware devices
If you are using GPIO-connected hardware enable and configure it here
### Dac
* _DAC Chip_ - Currently there is only one AK DAC chip supported and tested
* _DAC I2C Address_ - I2C address of the DAC
* _Digital filter_ - Select one of the digital filters supported by DAC
* _Gain level_ - Select one of the analog output levels provided by DAC
* _Sound settings_ - Select one of the sound profiles provided by DAC

### IR Remote control
* _Remote maker_ - Chose the remote Model you want to use (atm only one remote is supported)
* _LIRC socket path_ - The default value should work in most cases.


### OLED
* _Display model_ - Select OLED model (currently one supported)
* _SPI Device path_ - The default value should work in most cases

### Audio output selector
* Enable if you use output selection relay
 -------

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
 
## General

* [x] Show files as a tree
* [ ] Show media library (artist/album) as a tree
* [ ] Web radio browse/search/~~play~~
* [ ] Search files
* [ ] Playlist by genre
* [ ] Most played songs playlist
* [ ] Liked songs playlist
* [ ] New music mix dynamic playlist
* [ ] Like song
* [ ] Seek to position
* [ ] Keep last N songs in *history* when random mode is enabled
* [ ] Automatic library scan after music directory change, or time based scan
* [ ] Use fixed unique port instead 80 and fallback 8000
* [ ] Convert volume units to db
* [ ] Loudness limitter by BS1770
* [ ] Support more remote control models - configuration and key mapping
* [ ] Support more AK DAC models
* [ ] Mute relay
* [ ] DSP support (i.e. camillaDSP?)
* [ ] use more information about the song based on last.fm response, update id tags on local files?
* [ ] lyrics?
* [ ] analyze audio files for song matching and similarity (bliss-rs), create playlists from a song
* [ ] streaming to local device (i.e. phone) for i.e. preview
* [ ] convert PCM to DSD on the fly
<!-- * [ ] UPNP -->
  
## Queue page
* [x] Pagination
* [x] Manage items (batch, on search results): ~~clear~~, ~~delete~~, ~~play~~, playnext
 
## Playlist page
* [x] Manage selected playlist:
   * ~~play item~~
   * ~~add item(s) to the queue~~
   * play next
   * ~~replace queue with item(s)~~
   * delete playlist
* [x] Pagination
 
## Settings page
* [ ] Add all settings

## Code improvements
* [ ] get rid of `.unwrap()` calls
* [ ] get rid of unnecessary `.clone()` calls
* [ ] refactor names all over the code
* [ ] replace `warp` with `axum` or `actix`
* [ ] write generic fun and macros to reduce code duplication in UI

-------
 
# Development
 
## Setup development platform device - Raspberry PI 4 with RPI OS Lite 64-bit
 
## Setup OS
* update and change user pass (optional)
```bash
sudo apt update
sudo apt upgrade
passwd
sudo reboot
```
* copy ssh key
`ssh-copy-id pi@$RPI_HOST`
 
* install micro (optional)
```bash
curl https://getmic.ro | bash
sudo mv micro /usr/bin
```
 
* install zsh (optional)  https://github.com/ohmyzsh/ohmyzsh
```bash
sudo apt install -y zsh git fzf micro
sh -c "$(wget https://raw.githubusercontent.com/robbyrussell/oh-my-zsh/master/tools/install.sh -O -)"
git clone https://github.com/zsh-users/zsh-autosuggestions ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-autosuggestions
edit `~/.zshrc` 
 `plugins = (zsh-autosuggestions)`
alias rdp=sudo systemctl restart rsplayer
alias jdp=journalctl -u rsplayer.service -f -n 300
```
 
## Mount network share
```bash
sudo apt install -y nfs-common
sudo mkdir /media/nfs
sudo mount /media/nfs
mkdir /home/pi/music
ln -s /media/nfs/MUSIC /home/pi/remote
```
## Setup new remote
```bash
irdb-get download apple/A1156.lircd.conf
sudo cp A1156.lircd.conf /etc/lirc/lircd.conf.d
irrecord -d /dev/lirc0 dplayd.lircd.conf
sudo cp dplayd.lircd.conf /etc/lirc/lircd.conf.d
```

## Install build tools
```bash
cargo install cargo-binstall
cargo binstall cargo-make
cargo make install_tools
```


## Update Makefile.toml
set RPI_HOST to ip address of your device
 
## Build and copy backend to dev platform rpi
`cargo make copy_remote`
 
 ## Build and copy UI to dev platform rpi
```
cd rsplayer_web_ui
cargo make copy_remote
```

## Build on linux (local dev env)
* install build tools and deps for local build
`sudo apt install build-essintials pkg-config libasound2-dev`
* install cargo make
    ```
    cargo install cargo-binstall
    cargo binstall cargo-make
    ``` 
* local build/run (linux amd64)
`cargo make build_release` or `cargo make run_local`

### build for arm64 rpi

* install podman and pull image
`podman pull docker.io/ljufa/rsplayer-cross-aarch64:latest`
* build rsplayer
`cargo make build_release`
* build and copy to rpi device 
`cargo make copy_remote`  

... TODO ...
