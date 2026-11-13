#!/bin/bash
# macOS only (CI job `macos-bundle`): builds a .dmg around a stub binary, mounts it and checks
# the bundle.
set -eu
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# shellcheck disable=SC2016
printf '#!/bin/sh\nfor a in "$@"; do echo "$a"; done\n' > "$tmp/cecilia"
chmod +x "$tmp/cecilia"
sh packaging/macos/bundle.sh 0.0.1 "$tmp/cecilia" "$tmp/cecilia.dmg"

mkdir "$tmp/mnt"
hdiutil attach -nobrowse -readonly -mountpoint "$tmp/mnt" "$tmp/cecilia.dmg" >/dev/null
trap 'hdiutil detach -quiet "$tmp/mnt" || true; rm -rf "$tmp"' EXIT
app=$tmp/mnt/Cecilia.app
plutil -lint "$app/Contents/Info.plist" || fail "Info.plist"
grep -q '<string>0.0.1</string>' "$app/Contents/Info.plist" || fail "version not substituted"
[ -f "$app/Contents/Resources/cecilia.icns" ] || fail "no icon"
[ -L "$tmp/mnt/Applications" ] || fail "no Applications link"
[ "$("$app/Contents/MacOS/cecilia-gui" x)" = "$(printf -- '--gui\nx')" ] || fail "launcher"
codesign --verify "$app" || fail "signature"
echo "macos bundle: ok"
