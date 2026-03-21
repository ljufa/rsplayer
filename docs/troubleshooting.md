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
2. Enable **Set alsa buffer frame size (Experimental!)**.
3. Start with a value of **2000** and test playback.
4. If playback still breaks, increase gradually: **3000**, **4000**, **5000**, etc., until playback is stable.
5. Use the lowest value that gives stable playback, as larger buffers add more latency.

**Additional tips for low-spec hardware:**
- Increase the **Ring buffer size (ms)** — try values like 2000–5000 ms.
- Lower the **Player thread priority** if you notice the system becoming unresponsive.
- Avoid high-resolution files (e.g., 24-bit/192kHz) if your device struggles — standard 16-bit/44.1kHz is much less demanding.

## Playlist page is empty
TODO

### TODO...
