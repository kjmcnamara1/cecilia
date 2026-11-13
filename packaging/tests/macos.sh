#!/usr/bin/env bash
# The parts of the macOS bundle that can be checked without macOS: the iconset, the plist
# template and the launcher. bundle.sh itself is exercised by packaging/macos/test.sh on macOS.
set -eu
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# iconset made from the lily
sh packaging/macos/make-iconset.sh crates/cecilia-gui/assets/lily.svg "$tmp/i.iconset"
for n in 16x16 16x16@2x 32x32 32x32@2x 128x128 128x128@2x 256x256 256x256@2x 512x512 512x512@2x; do
    [ -f "$tmp/i.iconset/icon_$n.png" ] || fail "missing icon_$n.png"
done
file "$tmp/i.iconset/icon_512x512@2x.png" | grep -q '1024 x 1024' || fail "512@2x is not 1024 px"
file "$tmp/i.iconset/icon_16x16.png" | grep -q '16 x 16' || fail "16x16 is not 16 px"

# .dmg window: background renders at both sizes, settings name the files bundle.sh stages
for w in 640 1280; do
    rsvg-convert -w "$w" -o "$tmp/bg$w.png" packaging/macos/dmg-background.svg
    file "$tmp/bg$w.png" | grep -q "$w x $((w * 400 / 640))" || fail "background at $w px"
done
python3 -c "import sys; compile(open(sys.argv[1]).read(), sys.argv[1], 'exec')" packaging/macos/dmg-settings.py || fail "dmg-settings.py does not compile"
for s in '"Cecilia.app"' '"Applications"' 'defines["background"]' 'defines["icon"]'; do
    grep -qF "$s" packaging/macos/dmg-settings.py || fail "dmg-settings.py lacks $s"
done

# plist template
plist=packaging/macos/Info.plist.in
xmllint --noout "$plist" || fail "plist is not well-formed"
for s in '<string>cecilia-gui</string>' '@VERSION@' NSMicrophoneUsageDescription \
    '<string>gp3</string>' '<string>gp4</string>' '<string>gp5</string>' '<string>gp</string>' \
    '<string>gpx</string>' '<string>gabc</string>' '<string>ccc</string>' '<string>ccm</string>' \
    '<string>mid</string>' '<string>musicxml</string>'; do
    grep -qF "$s" "$plist" || fail "plist lacks $s"
done

# launcher: starts the window, drops -psn_*, keeps names with spaces
cp packaging/macos/cecilia-gui "$tmp/cecilia-gui"
# shellcheck disable=SC2016
printf '#!/bin/sh\nfor a in "$@"; do echo "$a"; done\n' > "$tmp/cecilia"
chmod +x "$tmp/cecilia" "$tmp/cecilia-gui"
[ "$("$tmp/cecilia-gui" -psn_0_12345 "my song.gp5")" = "$(printf -- '--gui\nmy song.gp5')" ] \
    || fail "launcher arguments"
[ "$("$tmp/cecilia-gui")" = "--gui" ] || fail "launcher without arguments"
echo "macos: ok"
