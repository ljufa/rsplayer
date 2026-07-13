#!/usr/bin/env bash
# Regenerates the self-hosted apt + dnf repos (https://ljufa.github.io/rsplayer-pkg)
# in a checkout of the ljufa/rsplayer-pkg Pages repo.
#
# Usage: update-repo.sh <assets-dir> <pages-dir>
#   <assets-dir>  directory with the release's *.deb / *.rpm files
#   <pages-dir>   clone of ljufa/rsplayer-pkg; existing packages there are kept
#                 (pruned to the last $KEEP versions) so the repos retain a
#                 rollback version across releases
#
# Environment:
#   GPG_HOMEDIR  gpg homedir holding the signing key (default: user's gpg)
#   KEY_ID       signing key fingerprint (default: the project key)
#   KEEP         versions to keep per package (default: 2)
#
# Requires: apt-utils (apt-ftparchive), createrepo-c, rpm (rpmsign), gnupg
set -euo pipefail

ASSETS=$(readlink -f "$1")
PAGES=$(readlink -f "$2")
KEY_ID=${KEY_ID:-AB004521B57A4B72F2AB2F86284128987382EB95}
KEEP=${KEEP:-2}
CONF_DIR=$(dirname "$(readlink -f "$0")")

gpg_sign() {
    gpg ${GPG_HOMEDIR:+--homedir "$GPG_HOMEDIR"} --batch --yes --local-user "$KEY_ID" "$@"
}

# ── apt repo (deb/) ─────────────────────────────────────────────
echo "==> apt: importing debs into pool"
for deb in "$ASSETS"/*.deb; do
    base=$(basename "$deb")
    case "$base" in
        # A suite has a single armhf slot; it gets the armv6 build (runs on
        # every 32-bit Pi incl. Zero/1). The armv7 deb stays a release asset.
        *_armhfv7.deb) continue ;;
        *_armhfv6.deb) base=${base%_armhfv6.deb}_armhf.deb ;;
    esac
    pkg=${base%%_*}
    dest="$PAGES/deb/pool/main/${pkg:0:1}/$pkg"
    mkdir -p "$dest"
    cp "$deb" "$dest/$base"
done

echo "==> apt: pruning pool to last $KEEP versions"
for dir in "$PAGES"/deb/pool/main/*/*/; do
    ls "$dir" | awk -F_ '{print $2}' | sort -uV | head -n -"$KEEP" |
    while read -r old; do
        echo "    pruning $(basename "$dir") $old"
        rm -f "$dir"/*_"${old}"_*.deb
    done
done

echo "==> apt: regenerating dists/stable"
cd "$PAGES/deb"
rm -rf dists
for arch in amd64 arm64 armhf riscv64; do
    mkdir -p "dists/stable/main/binary-$arch"
    apt-ftparchive --arch "$arch" packages pool > "dists/stable/main/binary-$arch/Packages"
    gzip -9 -c "dists/stable/main/binary-$arch/Packages" > "dists/stable/main/binary-$arch/Packages.gz"
done
apt-ftparchive -c "$CONF_DIR/apt-release.conf" release dists/stable > dists/stable/Release
gpg_sign --clearsign -o dists/stable/InRelease dists/stable/Release
gpg_sign --armor --detach-sign -o dists/stable/Release.gpg dists/stable/Release

# ── dnf repo (rpm/) ─────────────────────────────────────────────
echo "==> rpm: importing rpms"
mkdir -p "$PAGES/rpm"
cp "$ASSETS"/*.rpm "$PAGES/rpm/"
cd "$PAGES/rpm"

echo "==> rpm: pruning to last $KEEP versions"
# Two file-naming schemes are in play (server: rsplayer_<ver>_<arch>.rpm,
# desktop: rsplayer-desktop-<ver>-1.<arch>.rpm) — ask rpm for name/version.
index=$(for f in *.rpm; do echo "$(rpm -qp --qf '%{NAME} %{VERSION}' "$f" 2>/dev/null) $f"; done)
for name in $(awk '{print $1}' <<<"$index" | sort -u); do
    awk -v n="$name" '$1==n {print $2}' <<<"$index" | sort -uV | head -n -"$KEEP" |
    while read -r old; do
        echo "    pruning $name $old"
        awk -v n="$name" -v v="$old" '$1==n && $2==v {print $3}' <<<"$index" | xargs -r rm -f
    done
done

echo "==> rpm: signing packages"
# rpm's %__gpg default doesn't resolve on Ubuntu runners — point it at gpg
rpmsign --define "__gpg $(command -v gpg)" \
        --define "_gpg_name $KEY_ID" \
        ${GPG_HOMEDIR:+--define "_gpg_path $GPG_HOMEDIR"} \
        --addsign ./*.rpm

echo "==> rpm: regenerating repodata"
rm -rf repodata
createrepo_c --quiet .
gpg_sign --armor --detach-sign -o repodata/repomd.xml.asc repodata/repomd.xml

# ── shared ──────────────────────────────────────────────────────
echo "==> exporting public key + static files"
gpg ${GPG_HOMEDIR:+--homedir "$GPG_HOMEDIR"} --batch --export "$KEY_ID" > "$PAGES/rsplayer.gpg"
cp "$CONF_DIR/rsplayer.repo" "$PAGES/rpm/rsplayer.repo"
cp "$CONF_DIR/pages-index.html" "$PAGES/index.html"
touch "$PAGES/.nojekyll"

echo "==> done"
