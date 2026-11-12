# Subsonic API

RSPlayer answers the Subsonic API (version 1.16.1, with OpenSubsonic extensions), so third-party Subsonic music apps can browse your RSPlayer library and play its files on the phone or computer they run on. It is meant for apps such as Symfonium, DSub, Amperfy, play:Sub, Feishin and Supersonic.

The app plays the music itself: this is not a remote control for RSPlayer's own output. To control what RSPlayer plays, use the web UI.

## Turning it on

Settings, **Subsonic API**:

1. Switch on **Enable Subsonic API**.
2. Set a **Username** (default `rsplayer`) and a **Password**. The API stays off until a password is set.
3. Note the **Server address** shown below the fields, for example `http://192.168.1.20:8000`.

Changes take effect immediately, no restart needed. The API is not available in demo mode or in the Android app.

### Desktop app

The desktop app only accepts connections from the same device unless Settings, **Remote Access**, **Allow access from other devices** is on (takes effect after Restart RSPlayer). A phone app needs it on. The headless server always listens on the network.

## Setting up an app

In the app, add a Subsonic (or "Subsonic / Navidrome / OpenSubsonic") server with:

| Field | Value |
|-------|-------|
| Server URL | the address from settings, e.g. `http://192.168.1.20:8000` |
| Username | your Subsonic username |
| Password | your Subsonic password |

Apps that offer "legacy authentication" or "send password in plain text" work either way: RSPlayer accepts the plain password and the salted token form.

## Security

RSPlayer has no login on its web UI; the Subsonic password only guards the `/rest/` API. Over plain HTTP the password (or a token derived from it) travels with every request, so anyone who can watch your network traffic can reuse it. Use a password you don't use anywhere else, and keep the API off on networks you don't trust. RSPlayer also serves HTTPS (port 8143 by default) when the `TLS_CERT_PATH` and `TLS_CERT_KEY_PATH` environment variables point to a certificate and key.

## What works

| Area | Endpoints |
|------|-----------|
| System | `ping`, `getLicense`, `getOpenSubsonicExtensions` (`formPost`), `getUser`, `getMusicFolders` (one folder), `getScanStatus`, `startScan` (an incremental "Update library") |
| Browsing (tags) | `getArtists`, `getArtist`, `getAlbum`, `getSong`, `getGenres` |
| Browsing (folders) | `getIndexes`, `getMusicDirectory` (artist, then album, then songs) |
| Lists | `getAlbumList`, `getAlbumList2` (`newest`, `random`, `alphabeticalByName`, `alphabeticalByArtist`, `byYear`, `byGenre`, `frequent`; `recent` and `highest` also rank by play count), `getRandomSongs`, `getSongsByGenre`, `getStarred`, `getStarred2` |
| Search | `search2`, `search3` (an empty query lists the whole library page by page, which apps use to sync) |
| Media | `stream`, `download` (with range requests), `getCoverArt` |
| Playlists | `getPlaylists`, `getPlaylist`, `createPlaylist`, `updatePlaylist`, `deletePlaylist`; the Most Played and Liked playlists are listed read-only |
| Annotation | `star` / `unstar` (the song's like in RSPlayer), `scrobble` (counts a play, like playing in RSPlayer), `setRating` (accepted, ignored) |
| Info | `getArtistInfo`, `getArtistInfo2`, `getAlbumInfo`, `getAlbumInfo2`, `getSimilarSongs`, `getSimilarSongs2`, `getTopSongs`, `getNowPlaying` return empty answers |

Responses come as XML (the default), JSON (`f=json`) or JSONP (`f=jsonp&callback=...`). Every endpoint answers at `/rest/name` and `/rest/name.view`, by GET or by form POST.

## Limits

- One user. The user can stream, download and edit playlists; there is no user management.
- No transcoding: songs are sent as the original file, whatever bit rate or format the app asks for. The app must be able to play the format (DSD files, for example, few apps can). Tracks of a cue sheet album are sent as WAV.
- Songs from a [remote library (S3)](remote_library.md) are redirected to the storage, so the phone fetches them straight from there and must be able to reach it.
- Only songs can be starred; starring an album or artist is accepted and ignored.
- Cover art is sent in its stored size; the requested `size` is ignored.
- File sizes and bit rates are not reported (RSPlayer does not store them).
- RSPlayer stores no empty playlists: creating one needs at least one song, and removing the last song deletes it.
- No jukebox mode, podcasts, internet radio, shares, bookmarks, play queue sync, lyrics or chat over the API.
