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


### TODO:

#### Improvements
* replace warp with axum or actix
* reimplement spotify support including configuration
* better control over alsa device lock
* control over samba mount points
* make unit tests
* get rid of static openssl (maybe to replace with rustls)
* detect dsd signal from waveio(when they implement it diyaudio.com)

#### Features
* implement own player based on Symphonia
* own media management with advanced search
* use more information about song based on last.fm response. maybe update id tags on local files.
* lirycs
* analyze audio files for song matching and similarity
* streaming to local device (i.e. phone) for i.e. preview 
* queue/playlist management: play next, remove, add ...
* support more dac chips
* support mode oled models
* try different audio backends: pipewire, oss, jack ...
* convert PCM to DSD on the fly
* integrate more online streaming services

