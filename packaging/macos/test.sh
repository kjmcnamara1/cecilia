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
[ -f "$tmp/mnt/.DS_Store" ] || fail "no window layout (.DS_Store)"
[ -f "$tmp/mnt/.VolumeIcon.icns" ] || fail "no volume icon"
[ -f "$tmp/mnt/.background.tiff" ] || fail "no window background"
[ "$("$app/Contents/MacOS/cecilia-gui" x)" = "$(printf -- '--gui\nx')" ] || fail "launcher"
codesign --verify "$app" || fail "signature"

# the install script's macOS path, from a fake release holding this dmg
rel=$tmp/rel/download/v0.0.1
mkdir -p "$rel"
cp "$tmp/cecilia.dmg" "$rel/cecilia-v0.0.1-universal-apple-darwin.dmg"
(cd "$rel" && shasum -a 256 cecilia-v0.0.1-universal-apple-darwin.dmg > SHA256SUMS)
env HOME="$tmp/home" CECILIA_BASE_URL="file://$tmp/rel" CECILIA_VERSION=0.0.1 \
    CECILIA_APP_DIR="$tmp/apps" CECILIA_BIN_DIR="$tmp/bin" sh packaging/install.sh \
    || fail "install.sh"
[ -x "$tmp/apps/Cecilia.app/Contents/MacOS/cecilia-gui" ] || fail "app not installed"
[ "$("$tmp/bin/cecilia" a)" = a ] || fail "cecilia command"
echo "macos bundle: ok"
