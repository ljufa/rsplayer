# Flatpak packaging (self-hosted repo)

This directory holds the Flatpak packaging for the **desktop app** only. The
headless server keeps its deb/rpm/tgz/Docker channels — it needs systemd, a
system user, `mount()` and reboot/poweroff, which don't fit the Flatpak
sandbox.

The app is distributed from a **self-hosted flatpak repo** served by GitHub
Pages at <https://ljufa.github.io/rsplayer-flatpak> (source repo
`ljufa/rsplayer-flatpak`). The `build_flatpak` job in
`.github/workflows/cd.yml` builds the flatpak on every release tag and pushes
the updated OSTree repo there. Users install with:

```bash
flatpak install https://ljufa.github.io/rsplayer-flatpak/io.github.ljufa.rsplayer.flatpakref
```

and get updates through regular `flatpak update`. Runtime dependencies (the
GNOME platform) are still fetched from Flathub's repo — that is where runtimes
are hosted; only the app itself comes from the self-hosted repo.

Files:

- `io.github.ljufa.rsplayer.yml` — the flatpak-builder manifest. Builds from a
  staged copy of the source tree (`src/` next to the manifest), so releases
  need **no manifest edits** (no commit pinning, no asset checksums).
- `io.github.ljufa.rsplayer.desktop` / `io.github.ljufa.rsplayer.metainfo.xml`
  — desktop entry and AppStream metadata installed by the manifest. Bump the
  `<releases>` entry in the metainfo on each release.
- `stage-sources.sh` — stages tracked files + the pre-built `dist/web-ui` into
  a build directory; shared by `cargo make build_flatpak` and CI.
- `rsplayer.flatpakrepo` / `io.github.ljufa.rsplayer.flatpakref` /
  `pages-index.html` — static files copied into the published repo on each
  release (remote description, one-command install ref, landing page).
- `cargo-sources.json` — generated, not committed. See below.

## Regenerating cargo-sources.json

The build is offline, so all crates (including the git fork of `cpal`) must be
vendored. `cargo make flatpak_cargo_sources` regenerates it from the workspace
`Cargo.lock` when needed (self-contained venv under `target/flatpak/tools`);
CI does the equivalent with
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools):

```bash
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
    Cargo.lock -o PKGS/flatpak/cargo-sources.json
```

## Web UI source

The server embeds `dist/web-ui` via RustEmbed at compile time. Building the
Dioxus/WASM UI inside flatpak-builder is not practical (needs `dx` + wasm
toolchain offline), so `stage-sources.sh` copies a pre-built `dist/web-ui`
into the staged tree — build it first with `cargo make build_ui_release`
(CI downloads the `web_ui` artifact from the Build UI job instead).

## Local build & test

```bash
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08
cargo make build_ui_release   # once, for dist/web-ui
cargo make build_flatpak      # stage + build + install (user installation)
cargo make run_flatpak
```

(The flathub remote above only provides the GNOME runtime/SDK.)

If `flatpak-builder` is not installed natively, `build_flatpak` falls back to
the `org.flatpak.Builder` flatpak. Note: it may fail to find user-installed
SDKs ("org.gnome.Sdk ... not installed") unless `FLATPAK_USER_DIR` is passed
through — the cargo-make task already does this:

```bash
flatpak install --user flathub org.flatpak.Builder
flatpak run --env=FLATPAK_USER_DIR=$HOME/.local/share/flatpak \
    --command=flatpak-builder org.flatpak.Builder \
    --user --install --force-clean build-dir io.github.ljufa.rsplayer.yml
```

Lint after manifest/metainfo changes:

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
  flatpak install --user ./rsplayer-<version>.flatpak   # pulls the GNOME runtime
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

- Bit-perfect/exclusive ALSA output works out of the box through
  `--device=all`.
- The "Pipewire" playback device is offered via the runtime's ALSA→PulseAudio
  compatibility layer (`--socket=pulseaudio`, served by pipewire-pulse on the
  host). The runtime ships no `wpctl`, so the Pipewire *volume control* falls
  back to `pactl` in the sandbox.
- Music library access defaults to `xdg-music:ro` plus removable drives
  (`/media`, `/run/media`, `/mnt`, read-only); users grant other folders via Flatseal
  or `flatpak override --filesystem=...`. Symlinks only resolve inside the
  sandbox when their *target* path is also granted — a symlink in `~/Music`
  pointing to an ungranted location appears dangling and scans empty.
- Inside the sandbox the server disables network-share mounting and system
  power actions (detected via `/.flatpak-info`).
- App data (fjall databases, settings) lands in
  `~/.var/app/io.github.ljufa.rsplayer/config/rsplayer`.

## Publishing (CI)

One-time setup, already assumed by the workflow:

1. Create the GitHub repo `ljufa/rsplayer-flatpak` (public, can start empty
   with any initial commit).
2. Enable GitHub Pages for it: Settings → Pages → deploy from branch `main`,
   root (`/`).
3. In `ljufa/rsplayer` add an Actions secret `FLATPAK_REPO_TOKEN`: a
   fine-grained PAT with **Contents: read and write** on
   `ljufa/rsplayer-flatpak`.

Publishing is two-staged, gated on the release actually going public:

1. **On every release tag** the `build_flatpak` job in `cd.yml` builds the
   flatpak (x86_64) from the checked-out tag with the pre-built web UI
   artifact and uploads a single-file `.flatpak` bundle to the (draft,
   pre-release) GitHub release.
2. **When the release is promoted to a full release** (the `released` event —
   pre-releases don't count, same gate as the Docker image), the "Publish
   flatpak repo" workflow (`flatpak-repo.yml`) downloads that bundle, imports
   it into the Pages repo with `flatpak build-import-bundle`, refreshes
   appstream/static deltas with `flatpak build-update-repo` (pruned to the
   last two releases to keep the Pages repo small), copies in
   `rsplayer.flatpakrepo`, the `.flatpakref` and the landing page, and
   force-pushes the result as a single fresh commit (so the git history of
   the Pages repo does not grow with binary churn).

`flatpak-repo.yml` can also be dispatched manually with a tag to (re-)publish
that release's bundle, e.g. to seed the repo from an already-published
release.

### GPG signing

The repo is GPG-signed — flatpak refuses default (system-wide) installs from
non-GPG-verified remotes ("Can't pull from untrusted non-gpg verified
remote"). A dedicated signing key (`RSPlayer Flatpak Repo`, fingerprint
`AB004521B57A4B72F2AB2F86284128987382EB95`) lives in:

- GNUPG homedir `~/.local/share/rsplayer-flatpak-gpg` on the dev machine
  (**back this up** — losing it means re-keying and every user re-adding the
  remote),
- the `FLATPAK_GPG_PRIVATE_KEY` Actions secret (armored export) used by
  `flatpak-repo.yml` to sign commits and the repo summary,
- as `GPGKey=` (base64 public key) inside `rsplayer.flatpakrepo` and the
  `.flatpakref`, so clients verify automatically.

To rotate the key: generate a new one, update the secret, the `GPGKey=` lines
and the fingerprint in `flatpak-repo.yml`, then re-publish.

## Release checklist

1. Make sure `Cargo.toml` references cpal by **git** (ljufa/cpal), not a local
   `../cpal` path — push the fork first if it has local fixes.
2. Add the new `<release>` entry to `io.github.ljufa.rsplayer.metainfo.xml`.
3. Tag + push the release — CI does the rest (build, publish to the Pages
   repo, `.flatpak` bundle on the GitHub release).
