### Detailed documentation -> https://ljufa.github.io/rsplayer/
# RSPlayer - Diy-friendly Music Player for Raspberry PI (or any other arm Linux computer).
### Currently it works with *Spotify* and *Music Player Daemon* as backend players and provides a unique UI experience.
### Optionally you can connect the following input/output hardware to the GPIO header:
- #### *DAC board* for hardware volume control and other DAC settings like the sound quality and digital filter
- #### *Rotary encoder* for volume control and power on
- #### *IR Receiver* for player remote control
- #### *OLED display* for player state info
- #### *Relay* for output audio signal selection.

-----
## For quick start [jump to installation instructions](https://ljufa.github.io/rsplayer/#/?id=install-rsplayer)
----
## Demo videos
[![Demo video](https://img.youtube.com/vi/S-LKkZeLEQ0/hqdefault.jpg)](https://youtu.be/S-LKkZeLEQ0)
[![Demo video](https://img.youtube.com/vi/kH-_5-JRHrw/hqdefault.jpg)](https://youtu.be/kH-_5-JRHrw)
[![Demo video](https://img.youtube.com/vi/biqSZ9TTWOg/hqdefault.jpg)](https://youtu.be/biqSZ9TTWOg)

## Hardware requirements
Mandatory:
- Raspberry PI 4 - for best audio quality. It will work with other models as well.
 
Optional:
- Diy friendly AK44xx DAC board i.e. [Diyinhk](https://www.diyinhk.com/shop/audio-kits/), [JLSounds](http://jlsounds.com/products.html) ...
- USB to I2S converter board. i.e. [WaveIO](https://luckit.biz/), [Amanero](https://amanero.com/), [JLSounds](http://jlsounds.com/products.html) ...
- Infrared Receiver TSOP312xx. i.e. [TSOP31238](https://eu.mouser.com/ProductDetail/Vishay-Semiconductors/TSOP31238?qs=5rGgbCH0pB1jaK4I0GvRsw%3D%3D)
- A1156 Apple Remote Control
- Oled display ST7920 128x64 (from Amazon, Ebay ...)
- Rotary Encoder (from Amazon, Ebay ...)
- Headphone Amp board i.e. [Whammy](https://diyaudiostore.com/products/whammy-completion-kit?_pos=3&_sid=bf6542f23&_ss=r)
- Power Supply
- Metal Case
 
## Features
Without additional hardware:
* MPD or Spotify player controls: play, pause, next, prev, toggle shuffle
* View, manage, search playback queue
* Browse static and dynamic playlists
* Software volume control by Alsa
 
With additional hardware:
* Hardware volume control by DAC chip
* Infrared remote control: Play, Pause, Next, Prev, Volume Up/Down, Poweroff
* Volume control using knob/rotary encoder
* Oled display for song and player info
* Switch audio output between speakers and headphones
* Change DAC settings: digital filter, gain, sound profile
 
... and many [others planed](https://ljufa.github.io/rsplayer/#/?id=roadmap)
## Architecture
![Diagram](docs/dev/architecture-2022-09-05-1620.png)
## My Audio Streamer Implementation
**[KiCad files](docs/kicad/)** could be found here
![front](docs/dev/my_streamer_front_small.jpg)
![back](docs/dev/my_streamer_back_small.jpg)
![inside](docs/dev/my_streamer_inside_small.jpg)
 
