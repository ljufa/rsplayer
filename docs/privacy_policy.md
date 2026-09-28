# Privacy Policy

_Last updated: 2026-09-28_

This policy applies to the RSPlayer Android app (`de.rsplayer.app`) and the other RSPlayer applications (server, desktop). RSPlayer is free, open source software (https://github.com/ljufa/rsplayer) developed by Dragan Ljubojevic.

## Summary

RSPlayer does **not** collect, transmit, sell or share personal data with the developer or any third party for analytics, advertising or tracking. There are no accounts, no ads, no analytics SDKs and no crash reporting services. Everything the app stores stays on your device (or on your own RSPlayer server). Some features fetch information from public third-party services, listed under [Network connections](#network-connections).

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
- **Radio metadata**: for some stations the app may fetch now-playing information from the station's metadata provider (for example `api.streamabc.net`).
- **Internet radio directory**: browsing and searching stations in the Radio library, and loading details of your favorite stations, queries the public [Radio Browser](https://www.radio-browser.info/) API (`de2.api.radio-browser.info`). The country, language, tag or search text you choose is sent with the request. Station logos are loaded directly from the addresses listed in that directory.
- **Album cover lookup**: when a playing track has no embedded or folder cover art, the app sends its artist and album name to the [Last.fm](https://www.last.fm/) API (`ws.audioscrobbler.com`) to find a cover image, which is then loaded from Last.fm's image servers.
- **Lyrics**: when you open the lyrics panel, the artist, title, album and duration of the current track are sent to [LRCLIB](https://lrclib.net/) (`lrclib.net`).
- **Update check**: when the web interface loads, it asks the GitHub API (`api.github.com`) for the latest RSPlayer release to show an update notice, at most once a day per browser. Only the release number is requested; nothing about your installation is sent. The Android app skips this check.
- **Multiroom sync**: when you group devices, audio and control data travel directly between your own devices, encrypted, using the iroh/QUIC peer-to-peer library. If a direct connection is not possible, traffic may pass through an iroh relay server, which only forwards encrypted data and cannot read it.
- **Your own server**: if you connect the app to an RSPlayer server on your network, data flows between the two devices you control.

The album cover, lyrics, radio directory and update requests are made directly by the web interface in your browser (or in the app's built-in web view), so those services see the IP address of the device you are using and standard browser request information. No account or identifier is sent, and the address of your RSPlayer server is not sent as a referrer. Their own privacy policies apply.

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

This section covers the websites rsplayer.de, docs.rsplayer.de, stats.rsplayer.de and the live demo at demo.rsplayer.de. They are run by Dragan Ljubojevic as a private, non-commercial open source project. Contact: support@rsplayer.de.

### Hosting

The websites run on a rented virtual server. The hosting provider processes data only on RSPlayer's behalf, as a processor under Art. 28 GDPR. Everything the pages need (scripts, styles, fonts, images) is served from these servers, so your browser does not contact third-party services such as CDNs or font providers. The one exception is the live demo: it is a full RSPlayer web interface, so the lookups described under [Network connections](#network-connections) (album covers, lyrics, radio directory, update check) happen there too.

When you open a page, your browser sends your IP address and standard request information (browser, requested page) to the server. This is technically necessary to deliver the page. The web servers keep no access logs, so this information is not stored.

### Visit statistics

The websites rsplayer.de and docs.rsplayer.de count visits with [GoatCounter](https://www.goatcounter.com/), self-hosted at stats.rsplayer.de. No data goes to a third party.

- No cookies, and nothing is stored in your browser.
- Your IP address is never written to disk. To tell a new visit from a page reload, GoatCounter keeps IP address and browser only in memory, under a random ID, for up to 8 hours.
- Recorded per visit: the page, the referring site, browser and operating system, and the country derived from the IP address. Screen size, which GoatCounter collects by default, is removed before the request is sent. Clicks on some buttons (for example "Try the live demo" or an install tab) are counted the same way.
- The stored records cannot be linked to you. They are kept as long as the statistics are useful and are never sold or shared.

The live demo at demo.rsplayer.de and the RSPlayer applications do not use analytics.

### Legal basis

Delivering the pages and counting visits are based on legitimate interest (Art. 6(1)(f) GDPR). Delivering a page requires processing your IP address. Anonymous statistics show which pages, features and install methods are used, so the project can focus its limited time. They are collected in a way that stores nothing that identifies you.

### Your rights

Under the GDPR you have the right to access, rectification, erasure and restriction of processing, and the right to object to processing based on legitimate interest (Art. 15 to 21 GDPR). Because no data that identifies you is stored, a request usually cannot be matched to you (Art. 11 GDPR). You can still ask at support@rsplayer.de. You also have the right to lodge a complaint with a data protection supervisory authority, in particular in the EU member state where you live.

## Changes to this policy

Changes will be published on this page and the "Last updated" date will be revised. Significant changes will also be noted in the [release notes](release_notes.md).

## Contact

Questions or requests: support@rsplayer.de, or open an issue at https://github.com/ljufa/rsplayer/issues
