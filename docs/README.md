# RSPlayer

RSPlayer is a Linux-based music player designed for audiophile-grade local playback on devices like the Raspberry Pi.

## Features

- **Audio Formats**: FLAC, MP3, AAC, OGG Vorbis, WAV, AIFF, CAF, DSD (DSF/DFF), APE (Monkey's Audio)
- **Low Latency**: Direct ALSA or PipeWire output
- **Browser Playback**: Stream to your web browser
- **DSP**: Parametric EQ, filters, and presets
- **Loudness Normalization**: EBU R128 standard
- **Synchronized Lyrics**: LRCLIB integration
- **Network Storage**: SMB/CIFS and NFS mount management
- **Keyboard Shortcuts**: Full keyboard control for playback and navigation
- **Themes**: 10+ built-in themes with dark/light modes

## Known Limitations

- **DSD passthrough**: DSP, loudness normalization, and resampling are bypassed for DSD files. The bitstream is passed directly to the DAC.
- **Radio streams**: Loudness normalization is unavailable for internet radio (requires pre-scanned metadata). Seeking is not supported.
- **Supported formats**: Opus, WMA, WavPack, TTA are not supported.
- **Local Browser Playback**: The browser's native audio engine handles playback directly. DSP, loudness normalization, resampling, VU metering, and DSD are all unavailable in this mode.

## Getting Started

- [Installation](installation.md) — supported platforms, install methods, and verification
- [Configuration](configuration.md) — audio settings, volume control, and hardware integration
- [Troubleshooting](troubleshooting.md) — common issues and fixes

## Usage
### Player
TODO
### Queue page
TODO
### Playlist page
TODO
