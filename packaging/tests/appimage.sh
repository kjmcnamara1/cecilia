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

# Run as an AppImage, AppRun keeps a hidden desktop entry with the icon in the user's data
# folder, so docks match the window (app ID `cecilia`) to the lily.
mkdir -p "$tmp/ar/usr/share/icons/hicolor/scalable/apps" "$tmp/sys"
cp packaging/linux/cecilia.svg "$tmp/ar/usr/share/icons/hicolor/scalable/apps/"
image="$tmp/my apps/\$x%.AppImage"
run() { APPIMAGE=$image XDG_DATA_HOME=$tmp/data XDG_DATA_DIRS=$tmp/sys "$tmp/ar/AppRun" "$@"; }
[ "$(run song.gp5 < /dev/null)" = "song.gp5" ] || fail "arguments not passed through as an AppImage"
entry=$tmp/data/applications/cecilia-appimage.desktop
icon=$tmp/data/icons/hicolor/scalable/apps/cecilia-appimage.svg
[ -f "$entry" ] || fail "no desktop entry"
cmp -s packaging/linux/cecilia.svg "$icon" || fail "icon not installed"
grep -qx "Icon=$icon" "$entry" || fail "entry does not point at the icon"
grep -qx 'StartupWMClass=cecilia' "$entry" || fail "entry does not match the window's app ID"
grep -qx 'NoDisplay=true' "$entry" || fail "entry shows in menus"
grep -qxF "Exec=\"$tmp/my apps/\\\\\$x%%.AppImage\" --gui %F" "$entry" || fail "Exec not escaped: $(grep Exec= "$entry")"
grep -qxF "TryExec=$image" "$entry" || fail "TryExec wrong"
if command -v desktop-file-validate > /dev/null; then
    desktop-file-validate "$entry" || fail "invalid desktop entry"
fi
# A package's cecilia.desktop already gives the window its icon: the AppImage's entry goes.
mkdir -p "$tmp/sys/applications"
cp packaging/linux/cecilia.desktop "$tmp/sys/applications/"
run < /dev/null > /dev/null
[ ! -e "$entry" ] && [ ! -e "$icon" ] || fail "entry kept beside an installed cecilia.desktop"
echo "appimage: ok"
