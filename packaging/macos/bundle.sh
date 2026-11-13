#!/bin/sh
# Builds Cecilia.app around a cecilia binary and packs it into a .dmg. macOS only (needs
# rsvg-convert from librsvg, iconutil, codesign and hdiutil).
#   bundle.sh VERSION BINARY OUT.dmg
# The app opens the window (cecilia-gui adds --gui). It is only ad-hoc signed: not notarized.
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

ln -s /Applications "$tmp/stage/Applications"
rm -f "$OUT"
hdiutil create -volname Cecilia -srcfolder "$tmp/stage" -format UDZO -ov "$OUT"
