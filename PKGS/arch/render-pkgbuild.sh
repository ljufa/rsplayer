#!/usr/bin/env bash
# Renders PKGS/arch/<pkgname>/PKGBUILD.in into a ready PKGBUILD (plus its
# .install file) for one release. The per-arch source URLs point at the GitHub
# release tarballs; their sha256sums are computed from the local copies. The
# LICENSE checksum comes from this checkout, which must match the release tag.
# The result is what makepkg builds in CI.
#
# Usage: render-pkgbuild.sh <pkgname> <version> <assets-dir> <out-dir>
#   <pkgname>     rsplayer-bin | rsplayer-desktop-bin
#   <assets-dir>  directory with the release .tgz files
#
# An architecture whose tarball is missing from <assets-dir> is left out;
# exits 3 if none of the package's tarballs is there.
set -euo pipefail

PKGNAME=$1
VERSION=$2
ASSETS=$3
OUT=$4
HERE=$(dirname "$(readlink -f "$0")")
RELEASE_URL="https://github.com/ljufa/rsplayer/releases/download"

# Arch Linux arch name -> release asset suffix
case $PKGNAME in
    rsplayer-bin)
        asset=rsplayer
        arches="x86_64:amd64 aarch64:arm64 armv7h:armhfv7 armv6h:armhfv6 riscv64:riscv64"
        ;;
    rsplayer-desktop-bin)
        asset=rsplayer-desktop
        arches="x86_64:amd64 aarch64:arm64"
        ;;
    *) echo "unknown package: $PKGNAME" >&2; exit 1 ;;
esac

arch_list=""
sources=""
for pair in $arches; do
    carch=${pair%%:*}
    suffix=${pair#*:}
    file="${asset}_${VERSION}_${suffix}.tgz"
    if [ ! -f "$ASSETS/$file" ]; then
        echo "[render] $PKGNAME: no $file, leaving out $carch" >&2
        continue
    fi
    sum=$(sha256sum "$ASSETS/$file" | cut -d' ' -f1)
    arch_list="$arch_list '$carch'"
    # ${pkgver} stays literal so the PKGBUILD reads naturally
    sources="${sources}source_${carch}=(\"${RELEASE_URL}/\${pkgver}/${asset}_\${pkgver}_${suffix}.tgz\")
sha256sums_${carch}=('${sum}')
"
done

if [ -z "$arch_list" ]; then
    echo "[render] $PKGNAME: no release tarballs found in $ASSETS" >&2
    exit 3
fi

mkdir -p "$OUT"
block="arch=(${arch_list# })
${sources%$'\n'}"
license_sum=$(sha256sum "$HERE/../../LICENSE" | cut -d' ' -f1)
awk -v version="$VERSION" -v block="$block" -v license_sum="$license_sum" '
    /^@ARCH_SOURCES@$/ { print block; next }
    { gsub(/@VERSION@/, version); gsub(/@LICENSE_SHA256@/, license_sum); print }
' "$HERE/$PKGNAME/PKGBUILD.in" > "$OUT/PKGBUILD"
if [ -f "$HERE/$PKGNAME/$PKGNAME.install" ]; then
    cp "$HERE/$PKGNAME/$PKGNAME.install" "$OUT/"
fi
echo "[render] $OUT/PKGBUILD:$arch_list" >&2
