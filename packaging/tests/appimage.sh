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
echo "appimage: ok"
