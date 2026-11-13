#!/bin/sh
# Renders the Homebrew formula and cask for a published release into a tap checkout:
#   render.sh VERSION SHA256SUMS TAP_DIR
# SHA256SUMS is the release's checksum file. Fails, writing nothing, when an asset has no line.
set -eu
[ $# -eq 3 ] || { echo "usage: $0 VERSION SHA256SUMS TAP_DIR" >&2; exit 2; }
VERSION=$1
SUMS=$2
TAP=$3
HERE=$(cd "$(dirname "$0")" && pwd)

sum_of() {
    sum=$(grep " $1\$" "$SUMS" | cut -d' ' -f1 | head -1)
    [ -n "$sum" ] || { echo "no checksum for $1 in $SUMS" >&2; exit 1; }
    echo "$sum"
}

linux=$(sum_of "cecilia-v$VERSION-x86_64-unknown-linux-gnu.tar.gz")
arm=$(sum_of "cecilia-v$VERSION-aarch64-apple-darwin.tar.gz")
intel=$(sum_of "cecilia-v$VERSION-x86_64-apple-darwin.tar.gz")
dmg=$(sum_of "cecilia-v$VERSION-universal-apple-darwin.dmg")

mkdir -p "$TAP/Formula" "$TAP/Casks"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA_LINUX_X86_64@/$linux/" \
    -e "s/@SHA_DARWIN_ARM64@/$arm/" -e "s/@SHA_DARWIN_X86_64@/$intel/" \
    "$HERE/cecilia.rb.in" > "$TAP/Formula/cecilia.rb"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA_DMG@/$dmg/" \
    "$HERE/cecilia.cask.rb.in" > "$TAP/Casks/cecilia.rb"
