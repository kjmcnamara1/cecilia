#!/usr/bin/env bash
# packaging/appimage/build.sh with a stub linuxdeploy: the AppDir layout and the arguments.
set -eu
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

printf '#!/bin/sh\necho stub\n' > "$tmp/cecilia"; chmod +x "$tmp/cecilia"
cat > "$tmp/linuxdeploy" <<'SH'
#!/bin/sh
echo "$@" > "$STUB_LOG"
echo "VERSION=$VERSION" >> "$STUB_LOG"
touch "$LDAI_OUTPUT"
SH
chmod +x "$tmp/linuxdeploy"

STUB_LOG="$tmp/log" LINUXDEPLOY="$tmp/linuxdeploy" \
    sh packaging/appimage/build.sh 0.0.1 "$tmp/cecilia" "$tmp/out" || fail "build.sh failed"

[ -x "$tmp/out/AppDir/usr/bin/cecilia" ] || fail "no binary in the AppDir"
grep -q -- '--output appimage' "$tmp/log" || fail "no --output appimage"
grep -q 'VERSION=0.0.1' "$tmp/log" || fail "VERSION not exported"
for flag in --desktop-file --icon-file; do
    path=$(tr ' ' '\n' < "$tmp/log" | grep -A1 -- "$flag" | tail -1)
    [ -f "$tmp/out/$path" ] || fail "$flag points at a missing file: $path"
done
[ -f "$tmp/out/cecilia-v0.0.1-x86_64.AppImage" ] || fail "output not named cecilia-v0.0.1-x86_64.AppImage"

# The custom AppRun opens the window when started without arguments and without a terminal
# (a double-click), and passes arguments through otherwise.
apprun=$(grep -o -- '--custom-apprun=[^ ]*' "$tmp/log" | cut -d= -f2) || fail "no --custom-apprun"
[ -f "$tmp/out/$apprun" ] || [ -f "$apprun" ] || fail "custom AppRun missing: $apprun"
[ -f "$apprun" ] || apprun=$tmp/out/$apprun
mkdir -p "$tmp/ar/usr/bin"
cp "$apprun" "$tmp/ar/AppRun"
# shellcheck disable=SC2016
printf '#!/bin/sh\nfor a in "$@"; do echo "$a"; done\n' > "$tmp/ar/usr/bin/cecilia"
chmod +x "$tmp/ar/AppRun" "$tmp/ar/usr/bin/cecilia"
[ "$("$tmp/ar/AppRun" < /dev/null)" = "--gui" ] || fail "double-click does not open the window"
[ "$("$tmp/ar/AppRun" song.gp5 < /dev/null)" = "song.gp5" ] || fail "arguments not passed through"
echo "appimage: ok"
