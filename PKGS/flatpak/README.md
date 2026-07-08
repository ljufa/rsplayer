# Flatpak packaging (Flathub)

This directory holds the Flathub packaging for the **desktop app** only. The
headless server keeps its deb/rpm/tgz/Docker channels — it needs systemd, a
system user, `mount()` and reboot/poweroff, which don't fit the Flatpak
sandbox.

Files:

- `io.github.ljufa.rsplayer.yml` — the flatpak-builder manifest (source of
  truth; the copy in the `flathub/io.github.ljufa.rsplayer` repo must be kept
  in sync on every release).
- `io.github.ljufa.rsplayer.desktop` / `io.github.ljufa.rsplayer.metainfo.xml`
  — desktop entry and AppStream metadata installed by the manifest.
- `cargo-sources.json` — generated, not committed. See below.

## Regenerating cargo-sources.json

Flathub builds are offline, so all crates (including the git fork of `cpal`)
must be vendored. Generate from the workspace `Cargo.lock` with
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools):

```bash
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
    ../../Cargo.lock -o cargo-sources.json
```

Regenerate whenever `Cargo.lock` changes.

## Web UI source

The server embeds `dist/web-ui` via RustEmbed at compile time. Building the
Dioxus/WASM UI inside flatpak-builder is not practical (needs `dx` + wasm
toolchain offline), so the manifest consumes the pre-built
`web-ui-dist-<version>.tar.gz` release asset published by the "Full release"
workflow (`.github/workflows/cd.yml`). On each release, update the archive
URL and replace `__WEB_UI_DIST_SHA256__` with the asset's sha256.

For a local build before a release asset exists, build the UI
(`cargo make build_ui_release`) and swap the archive source for:

```yaml
      - type: dir
        path: ../../dist/web-ui
        dest: dist/web-ui
```

## Local build & test

```bash
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean build-dir io.github.ljufa.rsplayer.yml
flatpak run io.github.ljufa.rsplayer
```

If `flatpak-builder` is not installed natively, use the `org.flatpak.Builder`
flatpak. Note: it may fail to find user-installed SDKs ("org.gnome.Sdk ... not
installed") unless `FLATPAK_USER_DIR` is passed through:

```bash
flatpak install --user flathub org.flatpak.Builder
flatpak run --env=FLATPAK_USER_DIR=$HOME/.local/share/flatpak \
    --command=flatpak-builder org.flatpak.Builder \
    --user --install --force-clean build-dir io.github.ljufa.rsplayer.yml
```

Lint before submitting:

```bash
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.ljufa.rsplayer.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream io.github.ljufa.rsplayer.metainfo.xml
```

## Testing on other machines / VMs before publishing

`cargo make build_flatpak` leaves an OSTree repo at `target/flatpak/repo`.
Two ways to get it onto a test machine:

- **Single-file bundle**: `cargo make flatpak_bundle` produces
  `target/flatpak/rsplayer-<version>.flatpak`. Copy it to the VM and:
  ```bash
  flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  flatpak install --user ./rsplayer-<version>.flatpak   # pulls the GNOME runtime from Flathub
  ```
- **HTTP remote** (tests install/update the way real users get it):
  ```bash
  # on the build machine
  python3 -m http.server 8080 -d target/flatpak
  # on the VM
  flatpak remote-add --user --no-gpg-verify rsplayer-test http://<host-ip>:8080/repo
  flatpak install --user rsplayer-test io.github.ljufa.rsplayer
  ```
  After a rebuild, `flatpak update` on the VM picks up the new build.

The app payload is identical on every distro (that's the point of Flatpak);
what VM testing actually validates is host integration: PipeWire vs
PulseAudio, X11 vs Wayland session, menu entry/icon, portals and theming.

## Sandbox notes

- Bit-perfect/exclusive ALSA output works through `--device=all` (same as
  other hi-fi players on Flathub).
- The "Pipewire" playback device is offered via the runtime's ALSA→PulseAudio
  compatibility layer (`--socket=pulseaudio`, served by pipewire-pulse on the
  host). The runtime ships no `wpctl`, so the Pipewire *volume control* type
  is hidden in the sandbox — use Alsa or Software volume instead.
- Music library access defaults to `xdg-music:ro` plus removable drives
  (`/media`, `/run/media`, `/mnt`, read-only); users grant other folders via Flatseal
  or `flatpak override --filesystem=...`. Symlinks only resolve inside the
  sandbox when their *target* path is also granted — a symlink in `~/Music`
  pointing to an ungranted location appears dangling and scans empty.
- Inside the sandbox the server disables network-share mounting and system
  power actions (detected via `/.flatpak-info`).
- App data (fjall databases, settings) lands in
  `~/.var/app/io.github.ljufa.rsplayer/config/rsplayer`.

## Release checklist

0. Make sure `Cargo.toml` references cpal by **git** (ljufa/cpal), not the
   local `../cpal` path used during development — the Flathub build checks out
   the release tag and has no sibling checkout. Push the fork first if it has
   local fixes. (The local `cargo make build_flatpak` handles the path dep by
   staging `../cpal` into the tree; the canonical manifest does not.)
1. Tag + publish the GitHub release (includes `web-ui-dist-<ver>.tar.gz`).
2. Update the manifest: git tag + `__RELEASE_COMMIT__` (the tagged commit
   hash), web-ui archive URL + `__WEB_UI_DIST_SHA256__`; regenerate
   `cargo-sources.json` if `Cargo.lock` changed. Bump the `<releases>` entry
   in the metainfo.
3. Open a PR in `flathub/io.github.ljufa.rsplayer` with the updated manifest;
   the Flathub buildbot builds x86_64 + aarch64 and publishes on merge.
