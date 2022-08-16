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

##### Improvements
* get rid of `.unwrap()` calls
* refactor names all over the code
* replace warp with axum or actix
* better control over alsa device lock
* control over samba mount points
* make unit tests
* detect dsd signal from waveio(when they implement it diyaudio.com)

##### General
* implement own player based on Symphonia
* own media management with advanced search
* use more information about song based on last.fm response, update id tags on local files?
* lyrics
* analyze audio files for song matching and similarity
* streaming to local device (i.e. phone) for i.e. preview 
* support more dac chips
* support more oled models
* try different audio backends: pipewire, oss, jack ...
* convert PCM to DSD on the fly
* integrate more online streaming services


##### Player page
* Show playing context if exists: player type, playlist, album ...
* Show next playing song
* Like playing item button
* Seek to position
* Better style for control buttons


##### Queue page
<!-- * Show playing context: playlist, album, manual queue ... -->
<!-- * Search items  -->
* Manage items (batch, on search results): clear, delete, play, playnext
<!-- * Mark currently playing item -->
* Pagination
* Support Spotify podcast

##### Playlist page

* Search all playlists by name
* Show items of selected playlist
* Manage selected playlist:
    * play item
    * add item(s) to queue
    * play next
    * replace queue with item(s)
    * delete playlist
<!-- * Add more playing contexts (playlist types) provided by Spotify i.e. recommended, discover weekly... -->
* Pagination

##### Settings page
* Show modal wait window while server is restarting. use ws status
* Add all settings