#!/usr/bin/env bash
# Builds the Arch Linux packages (rsplayer-bin, rsplayer-desktop-bin) as
# .pkg.tar.zst for every architecture that has a release tarball, by running
# makepkg on the rendered PKGBUILDs. Nothing is compiled: the packages wrap the
# prebuilt release binaries, so one x86_64 Arch host packages all arches.
#
# Usage: build-pkgs.sh <version> <assets-dir> <out-dir>
#   <assets-dir>  directory with the release .tgz files
#   <out-dir>     receives the .pkg.tar.zst files, plus the rendered
#                 PKGBUILD/.SRCINFO per package under aur/<pkgname>/
#
# Runs on Arch Linux (CI: the archlinux container). Needs base-devel.
# makepkg refuses to run as root, so as root it builds as a "builder" user.
set -euo pipefail

VERSION=$1
ASSETS=$(readlink -f "$2")
mkdir -p "$3"
OUT=$(readlink -f "$3")
HERE=$(dirname "$(readlink -f "$0")")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

as_builder() {
    if [ "$(id -u)" -eq 0 ]; then
        runuser -u builder -- "$@"
    else
        "$@"
    fi
}

if [ "$(id -u)" -eq 0 ]; then
    id builder >/dev/null 2>&1 || useradd -m builder
    chown builder "$WORK" "$OUT"
fi

for pkg in rsplayer-bin rsplayer-desktop-bin; do
    dir="$WORK/$pkg"
    rc=0
    bash "$HERE/render-pkgbuild.sh" "$pkg" "$VERSION" "$ASSETS" "$dir" || rc=$?
    if [ "$rc" -eq 3 ]; then
        echo "[build] skipping $pkg" >&2
        continue
    elif [ "$rc" -ne 0 ]; then
        exit "$rc"
    fi
    # makepkg uses a source found next to the PKGBUILD instead of downloading
    # it, so the not yet published release tarballs are linked in.
    ln -s "$ASSETS"/*.tgz "$dir/"
    cp "$HERE/../../LICENSE" "$dir/LICENSE-$VERSION"
    if [ "$(id -u)" -eq 0 ]; then chown -R builder "$dir"; fi

    carches=$(cd "$dir" && as_builder bash -c 'source PKGBUILD && echo "${arch[@]}"')
    for carch in $carches; do
        # Package a foreign arch by overriding CARCH in a copy of makepkg.conf.
        # --nodeps: runtime deps are for the target system, not this build host.
        sed "s/^CARCH=.*/CARCH=\"$carch\"/" /etc/makepkg.conf > "$WORK/makepkg-$carch.conf"
        echo "[build] $pkg ($carch)" >&2
        (cd "$dir" && as_builder env PKGDEST="$OUT" makepkg --config "$WORK/makepkg-$carch.conf" --nodeps --force --cleanbuild --noconfirm)
    done

    mkdir -p "$OUT/aur/$pkg"
    (cd "$dir" && as_builder makepkg --printsrcinfo) > "$OUT/aur/$pkg/.SRCINFO"
    cp "$dir/PKGBUILD" "$OUT/aur/$pkg/"
    if [ -f "$dir/$pkg.install" ]; then cp "$dir/$pkg.install" "$OUT/aur/$pkg/"; fi
done

if [ "$(id -u)" -eq 0 ]; then chown -R builder "$OUT"; fi
ls -l "$OUT"/*.pkg.tar.zst
