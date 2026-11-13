#!/bin/sh
# Builds Cecilia.app around a cecilia binary and packs it into a .dmg. macOS only (needs
# rsvg-convert from librsvg, iconutil, tiffutil, codesign and dmgbuild: `pipx install dmgbuild`).
#   bundle.sh VERSION BINARY OUT.dmg
# The app opens the window (cecilia-gui adds --gui). It is only ad-hoc signed: not notarized.
# The .dmg window (background, icon places, volume icon) is set by dmg-settings.py and
# dmg-background.svg.
set -eu
[ $# -eq 3 ] || { echo "usage: $0 VERSION BINARY OUT.dmg" >&2; exit 2; }
VERSION=$1
BIN=$2
OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

app=$tmp/stage/Cecilia.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m755 "$BIN" "$app/Contents/MacOS/cecilia"
install -m755 "$HERE/cecilia-gui" "$app/Contents/MacOS/cecilia-gui"
sed "s/@VERSION@/$VERSION/g" "$HERE/Info.plist.in" > "$app/Contents/Info.plist"
sh "$HERE/make-iconset.sh" "$ROOT/crates/cecilia-gui/assets/lily.svg" "$tmp/cecilia.iconset"
iconutil -c icns -o "$app/Contents/Resources/cecilia.icns" "$tmp/cecilia.iconset"
codesign --force --deep -s - "$app"

# the window background at 1x and 2x, joined into one HiDPI tiff
rsvg-convert -w 640 -h 400 -o "$tmp/bg.png" "$HERE/dmg-background.svg"
rsvg-convert -w 1280 -h 800 -o "$tmp/bg@2x.png" "$HERE/dmg-background.svg"
tiffutil -cathidpicheck "$tmp/bg.png" "$tmp/bg@2x.png" -out "$tmp/bg.tiff"

rm -f "$OUT"
dmgbuild -s "$HERE/dmg-settings.py" -D app="$app" -D background="$tmp/bg.tiff" \
    -D icon="$app/Contents/Resources/cecilia.icns" Cecilia "$OUT"
