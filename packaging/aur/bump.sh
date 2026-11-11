#!/bin/sh
# Points both PKGBUILDs at a published release and fills in their checksums:
#   packaging/aur/bump.sh 0.45.0
# Run after the release workflow has published v0.45.0, then test with `makepkg` in each
# directory and push the two directories to the AUR (cecilia and cecilia-bin).
set -eu
VERSION=${1:?usage: $0 VERSION}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=kjmcnamara1/cecilia
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL -o "$TMP/src.tar.gz" "https://github.com/$REPO/archive/v$VERSION.tar.gz"
curl -fsSL -o "$TMP/SHA256SUMS" "https://github.com/$REPO/releases/download/v$VERSION/SHA256SUMS"
src=$(sha256sum "$TMP/src.tar.gz" | cut -d' ' -f1)
bin=$(grep "cecilia-v$VERSION-x86_64-unknown-linux-gnu.tar.gz" "$TMP/SHA256SUMS" | cut -d' ' -f1)
[ -n "$bin" ] || { echo "no x86_64 Linux tarball in SHA256SUMS" >&2; exit 1; }

for dir in cecilia:"$src" cecilia-bin:"$bin"; do
    sed -i -e "s/^pkgver=.*/pkgver=$VERSION/" -e "s/^pkgrel=.*/pkgrel=1/" \
        -e "s/^sha256sums=.*/sha256sums=('${dir#*:}')/" "$HERE/${dir%%:*}/PKGBUILD"
done
echo "PKGBUILDs set to $VERSION"
