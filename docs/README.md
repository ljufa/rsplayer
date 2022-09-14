Work in progress!
 
# Installing _(ssh access to rpi is required)_
- ### Raspberry PI configuration
  Tested on RPI4 with Raspberry Pi OS Lite (64-bit)
  - Enable SPI and I2C options using `raspi-config` tool
  - Also make sure you have following entries in `/boot/config.txt`:
     ```bash
     dtoverlay=gpio-ir,gpio_pin=17
     dtoverlay=rotary-encoder,pin_a=15,pin_b=18,relative_axis=1,steps-per-period=1
     gpio=18,15,19=pu
     gpio=22,23=op,dh
     ```
 
- ### Dependencies
  - Install MPD and LIRC:
      ```
      sudo apt install -y mpd lirc
      sudo systemctl enable mpd
      sudo systemctl enable lircd
      ```
  - [Librespot](https://github.com/librespot-org/librespot) is provided in the installation package
 
- ### RSPlayer
  Install rsplayer:
  ```
  wget https://github.com/ljufa/rsplayer/releases/download/0.3.2/rsplayer_0.3.2_arm64.deb
  sudo dpkg -i --force-overwrites rsplayer_0.3.2_arm64.deb
  sudo systemctl enable rsplayer
  ```
- ### Verify installation
  - Reboot RPI with `sudo reboot`
  - After reboot is done, open browser and navigate to `http://<rpi ip address>:8000/#settings`
  - If the page can not load check the log for errors with: `journalctl -u rsplayer.service -f -n 300`
 
-------
# Configuring
### MPD
### Spotify
### Active player
### Dac
### IR Remote control
### Volume control
### OLED
### Audio output selector
 
-------
# Usage
### Player
### Queue page
### Playlist page
 
-------
 
# Roadmap
### Improvements
* [ ] get rid of `.unwrap()` calls
* [ ] refactor names all over the code
* [ ] replace warp with axum or actix
* [ ] better control over alsa device lock
* [ ] control over samba mount points
* [ ] make unit tests
* [ ] detect dsd signal from waveio(when they implement it diyaudio.com)
 
### General
* implement own player based on Symphonia
* own media management with advanced search
* use more information about the song based on last.fm response, update id tags on local files?
* lyrics
* analyze audio files for song matching and similarity
* streaming to local device (i.e. phone) for i.e. preview
* support more dac chips
* support more oled models
* try different audio backends: pipewire, oss, jack ...
* convert PCM to DSD on the fly
* integrate more online streaming services
 
 
### Player page
* Show playing context if exists: player type, playlist, album ...
* Show next playing song
* Like playing item button
* Seek to position
* Better style for control buttons
 
 
### Queue page
<!-- * Show playing context: playlist, album, manual queue ... -->
<!-- * Search items  -->
* Manage items (batch, on search results): clear, delete, play, playnext
<!-- * Mark currently playing item -->
* Pagination
* Support Spotify podcast
 
### Playlist page
 
* Search all playlists by name
* Show items of the selected playlist
* Manage selected playlist:
   * play item
   * add item(s) to the queue
   * play next
   * replace queue with item(s)
   * delete playlist
<!-- * Add more playing contexts (playlist types) provided by Spotify i.e. recommended, discover weekly... -->
* Pagination
 
### Settings page
* Show modal wait window while the server is restarting. use ws status
* Add all settings
 
-------
 
# Developing
 
### Raspberry PI 4 with Ubuntu Server arm64 installation
 
#### Setup OS
* update and change user pass (optional)
```
sudo apt update
sudo apt upgrade
passwd
sudo reboot
```
 
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
```
 
#### Copy configuration
```bash
make copy_config
```
#### Mount network share
```bash
sudo apt install -y nfs-common
sudo mkdir /media/nfs
sudo mount /media/nfs
mkdir /home/pi/music
ln -s /media/nfs/MUSIC /home/pi/remote
```
#### Build librespot (optional)
```bash
cd github
git clone git@github.com:librespot-org/librespot.git && cd librespot
cp ../dplauer/Cross.toml .
cross build --target aarch64-unknown-linux-gnu --release --no-default-features --features alsa-backend
```
OR
```bash
make build_librespot
```
#### Install mpd
```bash
sudo apt install -y mpd
sudo systemctl enable mpd
```
#### LIRC setup
```bash
sudo apt-get install -y lirc
```
`/boot/config.txt (optional already provided in copy_config)`
```bash
dtoverlay=gpio-ir,gpio_pin=27
gpio=18,15,17=pu
gpio=22,23,9=op,dh
```
 
 
#### Enable and start dplay service
```bash
sudo systemctl enable dplay.service
sudo systemctl start dplay.service
```
#### setup new remote
```bash
irdb-get download apple/A1156.lircd.conf
sudo cp A1156.lircd.conf /etc/lirc/lircd.conf.d
irrecord -d /dev/lirc0 dplayd.lircd.conf
sudo cp dplayd.lircd.conf /etc/lirc/lircd.conf.d
```
 
#### Install LMS
```bash
wget http://downloads.slimdevices.com/nightly/8.2/lms/6e12028145512cef7d240c5d24c3b17e89ed8a6d/logitechmediaserver_8.2.0\~1609139175_arm.deb
sudo dpkg -i logitechmediaserver_8.2.0\~1609139175_arm.deb
sudo apt --fix-broken install
wget wget https://sourceforge.net/projects/lmsclients/files/squeezelite/linux/squeezelite-1.9.9.1372-aarch64.tar.gz/download
tar zxvf download
sudo cp squeezelite /home/ubuntu
squeezelite -V "Luckit Audio 2.0 Output" -o hw:CARD=L20,DEV=0 -C 1 -v -z
```
 
 
### Install build tools
`cargo install cross`
 
### Build release
 
`make release copytorpi`
 
### Features
#### Hardware integration
* `hw_oled` - enable control of OLED module over gpio spi protocol
* `hw_dac` - enable control of DAC chip, volume, filters, gain ...
* `hw_ir_control` - enable IR input based on LIRC
 
#### Backend player integrations
* `backend_mpd` - build with MPD - music player daemon integration  support
* `backend_lms` - build with LMS - Logitech Media Server integration support
 
 

