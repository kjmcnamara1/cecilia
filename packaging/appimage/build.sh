#!/bin/sh
# Builds cecilia-vVERSION-x86_64.AppImage in OUT_DIR from a cecilia binary:
#   build.sh VERSION BINARY OUT_DIR
# The AppDir is laid out by packaging/linux/stage.sh; linuxdeploy ($LINUXDEPLOY, default
# linuxdeploy-x86_64.AppImage on PATH) bundles the shared libraries and packs it. Run it with
# APPIMAGE_EXTRACT_AND_RUN=1 where FUSE is missing (CI).
set -eu
[ $# -eq 3 ] || { echo "usage: $0 VERSION BINARY OUT_DIR" >&2; exit 2; }
VERSION=$1
BIN=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)

mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
rm -rf "$OUT/AppDir"
"$HERE/../linux/stage.sh" "$OUT/AppDir" /usr "$BIN"

cd "$OUT"
export VERSION
export LDAI_OUTPUT="cecilia-v$VERSION-x86_64.AppImage"
"${LINUXDEPLOY:-linuxdeploy-x86_64.AppImage}" --appdir AppDir \
    --desktop-file AppDir/usr/share/applications/cecilia.desktop \
    --icon-file AppDir/usr/share/icons/hicolor/scalable/apps/cecilia.svg \
    --output appimage
