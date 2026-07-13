# Self-hosted apt + dnf repositories

GPG-signed deb/rpm package repositories served by GitHub Pages at
<https://ljufa.github.io/rsplayer-pkg> (source repo `ljufa/rsplayer-pkg`), so
users get RSPlayer updates through regular `apt upgrade` / `dnf upgrade`
instead of re-running `install.sh`. Same self-hosting pattern as the flatpak
repo (`PKGS/flatpak/README.md`).

Users add the repo once (see `pages-index.html` or the README) — `install.sh`
does exactly that on deb/rpm distros and falls back to a direct package
download if the repo is unreachable.

## Layout of the published Pages repo

```
index.html                    landing page with install instructions
rsplayer.gpg                  dearmored public signing key (apt keyring format)
deb/
  dists/stable/               InRelease, Release, Release.gpg,
                              main/binary-{amd64,arm64,armhf,riscv64}/Packages{,.gz}
  pool/main/r/rsplayer/       server debs
  pool/main/r/rsplayer-desktop/  desktop debs (amd64)
rpm/
  rsplayer.repo               dnf repo file (gpgcheck=1, repo_gpgcheck=1)
  repodata/                   createrepo_c metadata + repomd.xml.asc
  *.rpm                       all arches in one dir; dnf filters by arch
```

Notes:

- **armhf**: both 32-bit ARM builds declare Debian architecture `armhf`, and a
  suite has one armhf slot. The repo serves the **armv6** build (runs on every
  32-bit Pi including Zero/1); the armv7 deb remains a GitHub release asset
  only. RPM arches (`armv6hl`/`armv7hl`) are distinct, so both ship.
- **Retention**: the last 2 versions are kept per package (rollback via
  `apt install rsplayer=<version>` / `dnf downgrade rsplayer`); older ones are
  pruned to keep the Pages repo well under GitHub's size limits (~120 MB per
  release across all packages).

## Files here

- `update-repo.sh` — regenerates both repos inside a clone of
  `ljufa/rsplayer-pkg` from a directory of release `.deb`/`.rpm` assets:
  imports/prunes packages, rebuilds apt `dists/` (apt-ftparchive) and rpm
  `repodata/` (createrepo_c), signs everything, exports the public key and
  copies the static files. Used by CI and locally.
- `apt-release.conf` — apt-ftparchive Release metadata (origin, suite, arches).
- `rsplayer.repo` — dnf repo definition published at `rpm/rsplayer.repo`.
- `pages-index.html` — landing page published as `index.html`.

## Publishing (CI)

One-time setup, assumed by `.github/workflows/pkg-repo.yml`:

1. Create the GitHub repo `ljufa/rsplayer-pkg` (public, any initial commit).
2. Enable GitHub Pages for it: Settings → Pages → deploy from branch `main`,
   root (`/`).
3. In `ljufa/rsplayer` add an Actions secret `PKG_REPO_TOKEN`: a fine-grained
   PAT with **Contents: read and write** on `ljufa/rsplayer-pkg` (or extend
   the flatpak token to cover both repos and store it under this name too).

Publishing is gated the same way as the flatpak repo and the Docker image:
when a GitHub release is **promoted to a full release** (the `released` event
— the draft/pre-release created by `cd.yml` doesn't count), `pkg-repo.yml`
downloads the release's `.deb`/`.rpm` assets, runs `update-repo.sh` against a
clone of `ljufa/rsplayer-pkg`, and force-pushes the result as a single fresh
orphan commit (binary churn never accumulates in the Pages repo's history —
the previous versions kept for rollback live in the working tree of the clone
and are re-committed each time).

`pkg-repo.yml` can also be dispatched manually with a tag to (re-)publish that
release's packages, e.g. to seed the repo from an already-published release.

### GPG signing

The apt `Release` file, every rpm, and the rpm repo metadata are signed with
the **same project key as the flatpak repo** (fingerprint
`AB004521B57A4B72F2AB2F86284128987382EB95`, `FLATPAK_GPG_PRIVATE_KEY` secret —
see "GPG signing" in `PKGS/flatpak/README.md` for where the key lives and how
to rotate it; rotation additionally requires re-publishing this repo and users
re-downloading `rsplayer.gpg`). The dearmored public half is published as
`rsplayer.gpg` and referenced by the `signed-by=` sources.list option and the
`gpgkey=` line in `rsplayer.repo`.

## Local build & test

```bash
# throwaway signing key
export GPG_HOMEDIR=$(mktemp -d) KEY_ID=test@rsplayer
gpg --homedir "$GPG_HOMEDIR" --batch --passphrase '' --quick-gen-key "$KEY_ID"

mkdir -p /tmp/assets /tmp/pages
gh release download 4.6.0 --pattern '*.deb' --pattern '*.rpm' --dir /tmp/assets
PKGS/pkg-repo/update-repo.sh /tmp/assets /tmp/pages

# serve and install in containers
python3 -m http.server 8080 -d /tmp/pages &
docker run --rm -it --network=host debian:bookworm bash -c '
  apt update && apt install -y curl ca-certificates
  curl -fsSL -o /usr/share/keyrings/rsplayer.gpg http://localhost:8080/rsplayer.gpg
  echo "deb [signed-by=/usr/share/keyrings/rsplayer.gpg] http://localhost:8080/deb stable main" > /etc/apt/sources.list.d/rsplayer.list
  apt update && apt install -y rsplayer'
docker run --rm -it --network=host fedora:42 bash -c '
  curl -fsSL -o /etc/yum.repos.d/rsplayer.repo http://localhost:8080/rpm/rsplayer.repo
  sed -i "s|https://ljufa.github.io/rsplayer-pkg/rpm|http://localhost:8080/rpm|; s|https://ljufa.github.io/rsplayer-pkg/rsplayer.gpg|http://localhost:8080/rsplayer.gpg|" /etc/yum.repos.d/rsplayer.repo
  dnf install -y rsplayer'
```

Requires `apt-utils`, `createrepo-c`, `rpm` and `gnupg` on the machine running
`update-repo.sh`.
