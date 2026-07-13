# Snap packaging (Snap Store)

This directory holds the Snap Store packaging for the **desktop app** only —
the snap counterpart of `PKGS/flatpak`. The headless server keeps its
deb/rpm/tgz/Docker channels; mounting network shares and power actions don't
fit strict confinement, same as the flatpak.

Files:

- `../../snap/snapcraft.yaml` — the manifest. It lives at the repo root (not
  here) because `snapcraft` and `snapcore/action-build` require it there.
- `io.github.ljufa.rsplayer.desktop` — desktop entry installed by the
  manifest (`Exec=rsplayer` = the snap command; icon via `${SNAP}` path).
- `asound.conf` — loaded via `ALSA_CONFIG_PATH`; includes the staged
  `alsa.conf` and routes the ALSA "default" PCM to PulseAudio/PipeWire with
  a sysdefault fallback. (Not an `/etc/asound.conf` layout — SELinux on
  Fedora hosts denies snap-update-ns creating files under `/etc`.)

Unlike the flatpak there is no vendoring (`cargo-sources.json` equivalent):
snapcraft builds are online, so `cargo build --locked` fetches crates.io and
the `ljufa/cpal` git fork directly. The flatpak caveat still applies, though —
at release time `Cargo.toml` must reference cpal by **git**, not a local
`../cpal` path.

## Web UI

Same approach as the flatpak: the Dioxus/WASM UI is *not* built inside the
snap. `snapcraft` packs the source tree, so `dist/web-ui` just has to exist
before building:

- locally: `cargo make build_ui_release`
- in CI: the `build_snap` job downloads the `web_ui` artifact into
  `dist/web-ui` before running `snapcore/action-build`

The build fails fast with a clear error if `dist/web-ui/public/index.html`
is missing.

## Local build & test

```bash
cargo make build_ui_release          # once, or after UI changes
cargo make build_snap                # runs snapcraft (LXD backend)
cargo make install_snap              # sudo snap install --dangerous ./rsplayer_*.snap
snap run rsplayer                    # run from a terminal to see logs
```

`snapcraft` offers to install/configure LXD on first run. The build happens
in a clean core24 container, so local toolchain state doesn't leak in.
`build_snap` stages a clean copy of the tree under `target/snap/src` first
(git-tracked/untracked non-ignored files + `dist/`), because snapcraft
copies the entire source dir into the container and does not respect
`.gitignore` — building from the repo root would ship the multi-GB `target/`
caches. The resulting `rsplayer_*.snap` is moved to the repo root.

Interface connections to test the full feature set:

```bash
sudo snap connect rsplayer:alsa             # direct hw: (bit-perfect) output
sudo snap connect rsplayer:removable-media  # /media, /run/media, /mnt
```

`network`, `network-bind`, `audio-playback`, `home` and the gnome extension
plugs auto-connect.

## Sandbox notes

- Without `rsplayer:alsa` connected, only the virtual "Pipewire" playback
  device is offered (detected via the `SNAP_NAME` env var + the host pulse
  socket, mirroring the flatpak's `/.flatpak-info` detection). Playback goes
  through the `audio-playback` interface.
- With `rsplayer:alsa` connected, hw: cards enumerate and bit-perfect /
  exclusive output works, same as the flatpak's `--device=all`.
- `pactl` is staged (`pulseaudio-utils`), so the Pipewire *volume control*
  type stays available — unlike the flatpak runtime, which only has it
  because GNOME ships pactl.
- WebKitGTK's inner bubblewrap sandbox is disabled via
  `WEBKIT_FORCE_SANDBOX=0` plus `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`
  (WebKitGTK 2.46+ only honors the latter). It cannot nest inside snap
  confinement and has no snap auto-detection, unlike flatpak. Snap
  confinement still applies.
  If the window renders blank/garbled on some GPUs, the known WebKitGTK
  workarounds are `WEBKIT_DISABLE_DMABUF_RENDERER=1` /
  `WEBKIT_DISABLE_COMPOSITING_MODE=1` in `apps.rsplayer.environment`.
- The webview (WebKitGTK) is kept off xdg-desktop-portal, because on hosts
  where the portal cannot verify snap-confined callers (Fedora/SELinux:
  "Unable to open /proc/N/root" / "not available inside the sandbox")
  portal calls fail and WebKit then aborts the page load of the embedded
  server, showing the DBus error as the page. Three env vars handle it:
  `GTK_USE_PORTAL=0` (GTK file choosers) plus `GIO_USE_NETWORK_MONITOR=base`
  and `GIO_USE_PROXY_RESOLVER=dummy` — the latter two are the important
  ones, since the gnome-46 platform's GLib routes the network monitor and
  proxy resolver through the portal on *any* detected snap sandbox
  regardless of `GTK_USE_PORTAL`. The app only talks to its own localhost
  server, so forcing direct connections / no connectivity monitoring is
  safe. (On Ubuntu the portal identifies snaps via AppArmor and this
  doesn't bite, but the override is harmless there.)
- Network-share mounting and system power actions are disabled in the
  sandbox (detected via `SNAP_NAME` / the `RSPLAYER_DESKTOP` env).
- App data (fjall databases, settings) lands in
  `~/snap/rsplayer/current/.config/rsplayer` — snapd remaps `HOME`, so
  `dirs::config_dir()` resolves there (the analog of the flatpak's
  `~/.var/app/io.github.ljufa.rsplayer/config/rsplayer`).

## Store setup (one-time)

1. Create an Ubuntu One / Snap Store account, then register the name:
   `snapcraft register rsplayer` (fall back to `rsplayer-desktop` if taken —
   only the snap name changes, internal app/binary names stay).
2. Export CI credentials and add them as the `SNAPCRAFT_STORE_CREDENTIALS`
   GitHub secret, then delete the local file:
   ```bash
   snapcraft export-login --snaps rsplayer \
       --channels stable,candidate,beta,edge creds.txt
   ```
3. Optionally, after the first published release, request auto-connection of
   `alsa` (and `removable-media`) on <https://forum.snapcraft.io> citing
   other hi-fi players with raw ALSA output as precedent. Until granted,
   users connect manually (documented in the snap description and README).

## Release flow

The `build_snap` job in `.github/workflows/cd.yml` builds the snap on every
full release and, on tag builds, uploads it to the **candidate** channel.
The `.snap` file is also attached to the GitHub release. After smoke-testing
the candidate:

```bash
sudo snap install rsplayer --channel=candidate
# ...test...
snapcraft release rsplayer <revision> stable
```

(or promote via the store dashboard). Flip the CI `release:` input to
`stable` later if this proves reliable.

Version is read from the workspace `Cargo.toml` at build time — nothing to
bump here. There is no external repo or PR review to keep in sync; the
store listing (icon, description, screenshots) is managed with
`snapcraft` metadata plus the dashboard at <https://snapcraft.io/rsplayer>.
