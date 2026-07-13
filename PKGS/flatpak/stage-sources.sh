#!/usr/bin/env bash
# Stage a clean copy of the source tree for the flatpak dir-source build:
# all git-tracked + untracked (non-ignored) files, plus the gitignored
# pre-built web UI in dist/. Used by `cargo make build_flatpak` and by the
# build_flatpak job in .github/workflows/cd.yml.
#
# Usage: stage-sources.sh <dest-dir>   (run from the repo root)
set -euo pipefail

dest=${1:?usage: stage-sources.sh <dest-dir>}

test -f dist/web-ui/public/index.html || {
    echo 'dist/web-ui is missing — run: cargo make build_ui_release' >&2
    exit 1
}

rm -rf "$dest"
mkdir -p "$dest"
git ls-files -co --exclude-standard -z | tar --null -T - -cf - | tar -xf - -C "$dest"
cp -r dist "$dest/dist"
