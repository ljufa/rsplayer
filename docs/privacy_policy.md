# Privacy Policy

_Last updated: 2026-09-26_

This policy applies to the RSPlayer Android app (`de.rsplayer.app`) and the other RSPlayer applications (server, desktop). RSPlayer is free, open source software (https://github.com/ljufa/rsplayer) developed by Dragan Ljubojevic.

## Summary

RSPlayer does **not** collect, transmit, sell or share personal data with the developer or any third party for analytics, advertising or tracking. There are no accounts, no ads, no analytics SDKs and no crash reporting services. Everything the app stores stays on your device (or on your own RSPlayer server).

## Data stored on your device

The app stores the following locally, and only so it can work:

- **Settings**: audio output selection, DSP/equalizer settings, UI preferences, and similar configuration.
- **Music library index**: metadata (title, artist, album, cover art) read from the audio files you make available to the app.
- **Playback state**: queue, playlists, favorites, play history, and podcast subscriptions and progress.
- **Network share credentials**: if you configure an SMB/NFS share, its address and credentials are stored in the app's private storage and used only to connect to that share.

This data is never uploaded to the developer. Uninstalling the app or clearing its data deletes it.

## Permissions and why they are needed

| Permission | Purpose |
|---|---|
| Internet, network state, Wi-Fi state | Connect to your local network, internet radio, podcasts and the app's web interface |
| Multicast state | Discover other RSPlayer devices on your local network (multiroom) |
| Read media audio / images (or read external storage on older Android) | Read your music files and cover art to build the library |
| Foreground service (media playback), wake lock | Keep music playing with the screen off |
| Notifications | Show the playback notification and controls |
| Ignore battery optimizations | Prevent Android from stopping playback in the background |

The app does not access your contacts, location, camera, microphone, calendar, SMS or call logs.

## Network connections

The app only contacts the network when you use a feature that needs it:

- **Internet radio and podcasts**: the app connects to the stream and feed URLs you choose to play or subscribe to. Those servers will see your IP address and standard request information, as with any media player or browser.
- **Podcast search and artwork lookup**: search terms you enter may be sent to public directory services such as the Podcast Index (`api.podcastindex.org`) or Apple's iTunes Search API (`itunes.apple.com`). Their own privacy policies apply.
- **Radio metadata**: for some stations the app may fetch now-playing information from the station's metadata provider.
- **Multiroom sync**: when you group devices, audio and control data travel directly between your own devices, encrypted, using the iroh/QUIC peer-to-peer library. If a direct connection is not possible, traffic may pass through an iroh relay server, which only forwards encrypted data and cannot read it.
- **Your own server**: if you connect the app to an RSPlayer server on your network, data flows between the two devices you control.

No data is sent to the developer.

## Children

RSPlayer does not knowingly collect any information from anyone, including children under 13.

## Data sharing and retention

Because no personal data is collected by the developer, none is shared, sold or retained by the developer. Data on your device remains until you delete it.

## Security

Data is kept in the app's private storage on your device. Connections to third-party services use HTTPS wherever the service supports it. Some internet radio stations only offer plain HTTP streams, which is outside the app's control.

## License and disclaimer

RSPlayer is released under the [MIT License](https://github.com/ljufa/rsplayer/blob/main/LICENSE). The software is provided "as is", without warranty of any kind, express or implied. To the maximum extent permitted by law, the developer is not liable for any claim, damages or other liability arising from the use of the software, including any damage to audio equipment, speakers, hearing or data. See the full [Disclaimer](disclaimer.md).

## Websites

The websites rsplayer.de and docs.rsplayer.de count visits with [GoatCounter](https://www.goatcounter.com/), hosted on RSPlayer's own server at stats.rsplayer.de. No data goes to a third party.

- No cookies, and nothing is stored in your browser.
- Your IP address is never written to disk. To tell a new visit from a page reload, GoatCounter keeps IP address and browser only in memory, under a random ID, for up to 8 hours.
- Recorded per visit: the page, the referring site, browser and operating system, screen size, and the country derived from the IP address. Clicks on some buttons (for example "Try the live demo" or an install tab) are counted the same way.
- The data is used only to see which pages and install methods are used, and is never sold or shared.

The live demo at demo.rsplayer.de and the RSPlayer applications do not use analytics.

## Changes to this policy

Changes will be published on this page and the "Last updated" date will be revised. Significant changes will also be noted in the [release notes](release_notes.md).

## Contact

Questions or requests: support@rsplayer.de, or open an issue at https://github.com/ljufa/rsplayer/issues
