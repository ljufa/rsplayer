# Installing
* ### Install RPI OS
If you are going to install RPI os from scratch it is important to enable ssh, and wifi and specify the hostname.

  * For os image select _Raspberry PI OS Lite 64-bit_
  * Click on the gear icon and enable the following options

    ![](_assets/pi_imager_options.png ':size=450')




- ### Raspberry PI configuration  
  ?>This step is optional and it is only needed if you want to connect hardware devices to the GPIO header
  
  After installation is done ssh login to RPI `ssh pi@rsplayer.local` and make the following changes:
  - Enable SPI and I2C options using `raspi-config` tool
  - Make sure you have the following entries in `/boot/config.txt`:
     ```json
     dtoverlay=gpio-ir,gpio_pin=17
     dtoverlay=rotary-encoder,pin_a=15,pin_b=18,relative_axis=1,steps-per-period=1
     gpio=18,15,19=pu
     gpio=22,23=op,dh
     ```
 
- ### Install dependencies
  ?> Both MPD and LIRCare optional, don't install if you don't want to use MPD backend and LIRC based remote control
  - Install MPD and LIRC:
      ```bash
      sudo apt install -y mpd lirc
      sudo systemctl enable mpd
      sudo systemctl enable lircd
      ```
  - [Librespot](https://github.com/librespot-org/librespot) is provided in the installation package
 
- ### Install RSPlayer
  ```bash
  bash <(curl -s https://raw.githubusercontent.com/ljufa/rsplayer/master/install.sh)
  ```
- ### Verify installation
  - Reboot RPI with `sudo reboot`
  - After the reboot is done, open the browser and navigate to [http://rsplayer.local/](http://rsplayer.local/)
  - If you can't access to http://rsplayer.local from your android phone use RPI ip address or PC browser. At the time mDns/zeroconf is not supported by Android.
  - If the page can not load or there is an error message at top of the page please see the [Troubleshooting](?id=troubleshooting) section.
 
-------
# Configuring
## Players
To make any use of RSPlayer you need to enable and configure at least one player in the Players section.
To make configuration changes navigate to [http://rsplayer.local/#settings](http://rsplayer.local/#settings).
### MPD
* _Music Player Daemon server host_ - Default value assumes that you have MPD server running on the same host, change only if not true
* _Client port_ - MPD port, default value 6600


At this moment configuration of MPD through RSPlayer UI is not possible and has to be done manually by editing `/etc/mpd.conf` file. 
Here is an example:
```json
playlist_directory        "/var/lib/mpd/playlists"
db_file                   "/var/lib/mpd/tag_cache"
state_file                "/var/lib/mpd/state"
sticker_file              "/var/lib/mpd/sticker.sql"
music_directory           "/var/lib/mpd/music"

bind_to_address           "0.0.0.0"
port                      "6600"
log_level                 "default"
restore_paused            "yes"
auto_update               "yes"
follow_outside_symlinks   "yes"
follow_inside_symlinks    "yes"
zeroconf_enabled          "no"
filesystem_charset        "UTF-8"

input {
  plugin "curl"
}

audio_output {
  type                    "alsa"
  name                    "usb audio device"
  device                  "hw:1"
  mixer_type              "none"
  replay_gain_handler     "none"
}

```
### Spotify
?>Spotify integration is possible for Spotify premium accounts only. 

?>First time Spotify setup should be done from desktop computer because Andorid does not support accessing RPI using host name i.e `rsplayer.local`

!>_All credentials entered here, and generated Spotify access token will be stored in plain text format on your RPI device so please make sure it is properly secured!_

* _Spotify connect device name_ - you can provide your own name, it will be shown in the device list in official Spotify applications.
* _Spotify username_ - your Spotify account username
* _Spotify password_ - password for your Spotify account
* _Developer client id_ - If you don't own a Spotify developer account and you want to use mine please reach me in the private email message.
* _Developer secret_ - If you don't own a Spotify developer account and you want to use mine please reach me in the private email message.
* _Auth callback url_ - Change if your RPI hostname is different from `rsplayer.local`. Allowed values are:
  * http://raspberrypi.local/api/spotify/callback
  * http://raspberrypi.lan/api/spotify/callback
  * http://raspberrypi/api/spotify/callback
  * http://rsplayer.local/api/spotify/callback
  * http://rsplayer.lan/api/spotify/callback
  * http://rsplayer/api/spotify/callback
  * http://pi.local/api/spotify/callback
  * http://pi.lan/api/spotify/callback
  * http://pi/api/spotify/callback
  * http://localhost:8000/api/spotify/callback

Once you enter all values click _Authorize_ button which will show a permission popup from Spotify.
After giving permission you should see `Success` message and the close button.

### Audio device name 
This is an audio device that will be used by Librespot and MPD

### Active player
Here you should choose which (enabled and configured) player you want to use.

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

### Volume control
* _Volume control device_ - Select volume control device: Dac or Alsa
* _Volume step_ - How many units to send to the control device for a single button press or encoder step
* _Enable rotary encoder_ - Enable if you use a rotary encoder

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
## Useful commands
* get logs 
```bash
journalctl -u rsplayer.service -f -n 300
```
* restart rsplayer 
```bash
sudo systemctl restart rsplayer
```

## RSPlayer server can't start
TODO

## Can't connect to MPD error
TODO

## Playlist page is empty
TODO

## Spotify configuration
### Callback url not valid

### Developer client id not valid

### TODO...

-------

# Roadmap
 
## General

* [ ] Convert volume units to db
* [ ] Loudness limitter by BS1770
* [ ] Allow seamless play from different sources at runtime, i.e. create playlist/queue from spotify song, local library, radio, youtube songs ...
* [ ] Browse/search whole music library
* [ ] Web radio browse/search/play
* [ ] LMS backend support
* [x] implement own player based on Symphonia
* [ ] Support more remote control models - configuration and key mapping
* [x] MPD Configuration using RSPlayer UI (partially implemented for music dir and audio device)
* [ ] Support more AK DAC models
* [ ] Mute relay
* [ ] integrate more online streaming services. Qobuz, Tidal, Soundcloud ...
* [ ] DSP support (i.e. camillaDSP?)
* [ ] own media management with advanced search
* [ ] use more information about the song based on last.fm response, update id tags on local files?
* [ ] lyrics
* [ ] analyze audio files for song matching and similarity (bliss-rs), create playlists from song
* [ ] streaming to local device (i.e. phone) for i.e. preview
* [ ] convert PCM to DSD on the fly
<!-- * [ ] UPNP -->
 
 ## Player page
* [ ] Seek to position
* [x] Better design, show player control at the bottom of all pages
* [ ] Show playing context if exists: player type, playlist, album ...
* [ ] Show the next playing song
* [ ] Like playing item button
  
## Queue page
* [x] Pagination
* [x] Manage items (batch, on search results): ~~clear~~, ~~delete~~, ~~play~~, playnext
* [ ] Support Spotify podcast
 
## Playlist page
* [ ] Search all playlists by name
* [x] ~~Show items of the selected playlist~~
* [x] Manage selected playlist:
   * ~~play item~~
   * ~~add item(s) to the queue~~
   * play next
   * ~~replace queue with item(s)~~
   * delete playlist
* [x] Pagination
 
## Settings page
* [ ] Show modal wait window while the server is restarting. use ws status
* [ ] Add all settings

## Code improvements
* [x] Move configuration to human readable yaml file and make it backward compatible (migration support)
* [x] migrate away from `failure` crate
* [ ] get rid of `.unwrap()` calls
* [ ] refactor names all over the code
* [ ] replace `warp` with `axum` or `actix`
* [ ] better control over alsa device lock
* [ ] control over network shares
* [x] use separate message channel for system commands: volup, voldown, poweroff, restart ...
* [ ] write generic fun and macros to reduce code duplication in UI

-------
 
# Developing
 
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
sudo apt install -y zsh git fzf
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
 
## Install LMS
```bash
wget http://downloads.slimdevices.com/nightly/8.2/lms/6e12028145512cef7d240c5d24c3b17e89ed8a6d/logitechmediaserver_8.2.0\~1609139175_arm.deb
sudo dpkg -i logitechmediaserver_8.2.0\~1609139175_arm.deb
sudo apt --fix-broken install
wget wget https://sourceforge.net/projects/lmsclients/files/squeezelite/linux/squeezelite-1.9.9.1372-aarch64.tar.gz/download
tar zxvf download
sudo cp squeezelite /home/ubuntu
squeezelite -V "Luckit Audio 2.0 Output" -o hw:CARD=L20,DEV=0 -C 1 -v -z
```

## Install build tools
```bash
cargo install cargo-binstall
cargo install make
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
