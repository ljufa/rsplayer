# RSPlayer

RSPlayer is a Linux-based music player designed for audiophile-grade local playback on devices like the Raspberry Pi.

## Known Limitations

- **DSD passthrough**: DSP, loudness normalization, and resampling are bypassed for DSD files. The bitstream is passed directly to the DAC.
- **Radio streams**: Loudness normalization is unavailable for internet radio (requires pre-scanned metadata). Seeking is not supported.
- **Supported formats**: Decoding is handled by [Symphonia](https://github.com/pdeljanov/Symphonia). Opus, WMA, WavPack, APE, TTA, and other formats not supported by Symphonia will not play.
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
