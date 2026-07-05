# Troubleshooting
?>If you can't access http://rsplayer.local from your android phone use RPI ip address or PC browser. At the time mDns/zeroconf is not supported by Android.

## Useful commands
* get logs
```bash
journalctl -u rsplayer.service -f -n 300
```
* restart rsplayer
```bash
sudo systemctl restart rsplayer
```
Configuration settings are managed through the web UI and stored internally in a database under `/opt/rsplayer/rsplayer.db/`. The environment variables (ports, TLS, logging) are configured in `/opt/rsplayer/env`.

## Music library scan finds no files from a mounted drive (Samba/NFS)

> **Tip:** As of v2.5.0, rsplayer can mount and manage SMB/NFS shares directly from the Settings page under **Music Sources > Network Mounts**. Mounts created through the UI are automatically configured with the correct permissions. The manual steps below apply to mounts configured outside of rsplayer (e.g., via `/etc/fstab`).

If the library scan completes immediately with no files found, it is most likely a file permission issue. The `rsplayer` service runs as the `rsplayer` user, which may not have read access to your mount point.

**Verify the issue:**
```bash
sudo -u rsplayer ls /mnt/your-mount-path
```
If this returns "Permission denied", the `rsplayer` user cannot access the mount.

**Fix for Samba (CIFS) mounts:**

Edit your `/etc/fstab` entry and add `uid` and `gid` options so the `rsplayer` user owns the mounted files:
```
//server/share /mnt/samba/music cifs credentials=/etc/samba/creds,uid=rsplayer,gid=rsplayer,file_mode=0644,dir_mode=0755 0 0
```
Then remount and restart:
```bash
sudo mount -a
sudo systemctl restart rsplayer
```

**Fix for NFS mounts:**

Use `all_squash` with `anonuid`/`anongid` on the NFS server side, or add the `rsplayer` user to a group that has read access:
```bash
sudo usermod -aG <mount-group> rsplayer
sudo systemctl restart rsplayer
```

**Also check:**
- The **Music directory path** in RSPlayer settings matches the exact mount path where your audio files are located.
- The `cifs-utils` package is installed if using Samba (`sudo apt install cifs-utils`).

## RSPlayer fails to start because the port is already in use

If `rsplayer` fails to start and the logs show an "address already in use" error, another service (e.g., Apache, Nginx, or another web server) is already using the configured port.

**Check what is using the port:**
```bash
sudo ss -tlnp | grep ':80'
```

**Fix — change the RSPlayer port:**

Edit the environment file `/opt/rsplayer/env` and set `PORT` and/or `TLS_PORT` to available ports:
```
PORT=8080
TLS_PORT=8443
```

You can also restrict the bind address using `BIND_ADDR` (default `0.0.0.0`). For example, to listen only on loopback:
```
BIND_ADDR=127.0.0.1
```

Then restart the service:
```bash
sudo systemctl restart rsplayer
```

You will now access the web UI at `http://your-machine-ip:8080` instead of the default `http://your-machine-ip`.

**Alternative — stop the conflicting service:**
```bash
sudo systemctl stop <conflicting-service>
sudo systemctl disable <conflicting-service>
sudo systemctl restart rsplayer
```

## Audio playback stutters or breaks on low-spec hardware

On lower-powered devices like the Raspberry Pi Zero, Pi 2, or Pi 3A+, playback may stutter, skip, or stop entirely due to the default ALSA buffer size being too small for the hardware to keep up. You can confirm this issue by checking the logs — look for ALSA poll errors:
```bash
journalctl -u rsplayer.service -f -n 300
```

**Fix — increase the ALSA buffer frame size:**

1. Open the RSPlayer settings page in your browser.
2. Under Playback → Advanced, set **ALSA buffer size (frames, 0=default)**.
3. Start with a value of **2000** and test playback.
4. If playback still breaks, increase gradually: **3000**, **4000**, **5000**, etc., until playback is stable.
5. Use the lowest value that gives stable playback, as larger buffers add more latency.

**Additional tips for low-spec hardware:**
- Increase the **Ring buffer size (ms)** — try values like 2000–5000 ms.
- Lower the **Player thread priority** if you notice the system becoming unresponsive.
- Avoid high-resolution files (e.g., 24-bit/192kHz) if your device struggles — standard 16-bit/44.1kHz is much less demanding.

## Windows-specific issues

### Web UI shows a blank page in Microsoft Edge

Edge's **Enhanced Security Mode** (ESM) can silently disable WebAssembly JIT compilation for local sites, causing a blank page while Chrome works fine.

**Diagnose:** open Edge DevTools (F12) → Console tab → reload the page. If you see a message about WebAssembly being blocked, ESM is the cause.

**Fix:** add `localhost` as an exception in Edge:

1. Go to **Settings → Privacy, search, and services → Enhanced security on the web**.
2. Under **Exceptions**, add `http://localhost:8000`.

Alternatively, toggle Enhanced Security Mode off entirely for local development.

### Windows Firewall blocks the server port

The first time `rsplayer.exe` binds a port, Windows Firewall shows a permission dialog. If you dismissed it, the port is blocked.

**Fix:** run the following in an elevated (Administrator) terminal:

```powershell
netsh advfirewall firewall add rule name="RSPlayer" dir=in action=allow protocol=TCP localport=8000
```

### ASIO device is missing from the Audio interface list

ASIO drivers only appear (as `… (ASIO)` entries) when **both** conditions hold:

- The build includes the `asio` feature. Official Windows builds do; a self-compiled binary must be built with `--features asio` (see [build.md](build.md#windows-build-native)).
- The device's ASIO driver is installed and its control panel opens successfully. Devices that only expose WASAPI show up under their WASAPI names instead.

Because the device list is enumerated once at startup, an ASIO driver installed *after* launch will not appear until you **restart RSPlayer**.

### Playback briefly stops when a new ASIO driver is scanned

ASIO drivers are exclusive (single-client), so probing them can interrupt whatever is currently playing. RSPlayer avoids this by enumerating devices only at startup and reusing the cached list, so opening Settings during playback is safe. A deliberate device rescan (`GET /api/settings?rescan=true`) still re-probes the drivers and may cause a momentary dropout — trigger it while playback is stopped.
