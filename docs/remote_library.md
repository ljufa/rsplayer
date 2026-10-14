# Remote Library (S3)

RSPlayer can play a music library that lives on S3-compatible storage instead of a local folder. Nothing is mounted: the player lists the files, reads their tags with small ranged downloads and streams songs while they play. This works on every platform, including the Android app, which cannot mount network shares.

Supported storage: anything that speaks the S3 API, for example Amazon S3, Backblaze B2, Cloudflare R2, Wasabi, MinIO, Garage, or a plain folder published with `rclone serve s3`.

## Adding a remote library

Settings, Music Library, **Remote Libraries (S3)**:

| Field | Example | Notes |
|-------|---------|-------|
| URL | `https://music.example.com/music` | Server, then the bucket, then optionally a folder in it |
| Access key / Secret key | | Use a read-only key |
| Name | `music` | Optional: generated from the bucket and folder when empty |
| Advanced: Region | *(auto)* | Rarely needed, see below |

URL forms:

| Storage | URL |
|---------|-----|
| `rclone serve s3`, MinIO, Garage, any S3 server | `https://host[:port]/bucket[/folder]` (with `rclone serve s3`, the bucket is a folder directly under the served directory) |
| Amazon S3 | `s3://bucket[/folder]` or `https://bucket.s3.<region>.amazonaws.com[/folder]` |
| Backblaze B2 | `https://s3.<region>.backblazeb2.com/bucket[/folder]` |
| Cloudflare R2 | `https://<account>.r2.cloudflarestorage.com/bucket[/folder]` |
| Wasabi | `https://s3.<region>.wasabisys.com/bucket[/folder]` |

The region is taken from the host name where the provider puts it there (AWS, B2, Wasabi; `auto` for R2). Otherwise RSPlayer asks the server when it connects and falls back to `us-east-1`, which rclone, MinIO and Garage accept. Set it under Advanced only for a provider that needs a specific region and does not report it.

Use **Test connection** before adding. Then press **Update library**: the first scan downloads the beginning of every file (tags and embedded artwork), so a large library takes a while over a slow connection. Later updates only read new files.

Notes:

- If the storage cannot be reached during an update, the library's songs are kept as they are.
- Removing a remote library, or turning it off, removes its songs on the next update.
- APE files and SACD ISO images are not supported from remote storage yet.
- Loudness normalization does not analyse remote songs (it would have to download them in full).
- The secret key is never sent back to the web UI.

## Example: NAS library playable from anywhere, through a VPS

The NAS needs no open ports: it connects out to a VPS over WireGuard, and the VPS publishes the S3 gateway over HTTPS.

```
phone (RSPlayer) --HTTPS--> VPS (Caddy/Traefik) --WireGuard--> NAS (rclone serve s3) --> /volume1/music
```

### 1. S3 gateway on the NAS

With the music in `/volume1/music`, serve its parent so `music` becomes the bucket:

```bash
rclone serve s3 /volume1 \
  --auth-key "rsplayer,<a long random secret>" \
  --addr 10.8.0.2:8333 \
  --read-only \
  --etag-hash= \
  --buffer-size 0
```

`--etag-hash=` matters: by default rclone computes an MD5 of every file it lists, which reads the whole library from disk on each update (a 19,000-file library did not finish listing in 10 minutes; without hashing it took 36 seconds). RSPlayer does not use ETags. `--buffer-size 0` turns off rclone's read-ahead (16 MB per opened file by default): a library scan reads only the tags at the start and end of each file, several files at once, and with read-ahead rclone read about 13 times more from disk than asked, which made scanning a disk shared with other services about 4 times slower. If your music folder is a symlink inside the served directory, add `--copy-links`.

`10.8.0.2` is the NAS's WireGuard address, so the gateway is not reachable from the home LAN or the internet directly. Run it as a service (systemd unit or the NAS's task scheduler).

### 2. WireGuard tunnel from the NAS to the VPS

VPS `/etc/wireguard/wg0.conf`:

```ini
[Interface]
Address = 10.8.0.1/24
ListenPort = 51820
PrivateKey = <vps private key>

[Peer]
PublicKey = <nas public key>
AllowedIPs = 10.8.0.2/32
```

NAS `/etc/wireguard/wg0.conf`:

```ini
[Interface]
Address = 10.8.0.2/24
PrivateKey = <nas private key>

[Peer]
PublicKey = <vps public key>
Endpoint = <vps public ip>:51820
AllowedIPs = 10.8.0.1/32
PersistentKeepalive = 25
```

### 3. HTTPS on the VPS

`/etc/caddy/Caddyfile` (Caddy gets the certificate from Let's Encrypt automatically):

```
music.example.com {
    reverse_proxy 10.8.0.2:8333
}
```

Caddy passes the original `Host` header through, which S3 signatures require. With another reverse proxy, make sure it keeps the `Host` header unchanged.

### 4. In RSPlayer

URL `https://music.example.com/music` and the key pair from step 1.
