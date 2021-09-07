### Install build tools
`cargo install cross`

### Build release

`make release copytorpi`


### TODO:
#### Configuration
* configurable spotifyd/mpd/lms and audio device.
* configurable DAC
* setup/config UI
* dac and ir control optional/configurable.

#### UI
* rest backend for UI
* web UI
* android UI

#### General
* better control over alsa device lock.
* recover threads from panics.
* control over samba mount points.
* make unit tests
* get rid of static openssl to reduce size of the binary.
* detect dsd signal from waveio(when they implement it diyaudio.com)


#### Requirements
#### Startup sequence
* create last used player
    * if can't be reached in specified time, try create next one untill last one is reached
    * if none the players is created show error on display and exit the program, systemd will try start it again.
    * if one of the players created sucessfuly continue the start sequence. 

#### Normal operation
* User press play/stop/next/prev button
    * get active player reference, and call operation with specified timeout
    * if player can't finish within the specified time, try recovering target service and repeat play once more with timeout, if it happen again fail command handling with error (message to switch to next player)

* User press next player button:
    * stop current player, or maybe better all of them in the case play started from external application.
    * wait for alsa to unlock device
    * try creating next player with timeout, if not possible try next one until last one is reached.
    * if none of the players can't be created display error and fail command handling.
    * if next player is sucessfully created, execute play command on it.


