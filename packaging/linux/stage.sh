#!/bin/sh
# Lays cecilia out under DESTDIR/PREFIX: the binary, desktop entry, icon, file types, licences
# and docs. install.sh and the distribution packages (PKGBUILDs, .deb, .rpm) all use this, so
# they share one layout. Works from a source checkout and from a release tarball (both have
# LICENSE-*, README.md and CHANGELOG.md one level above this directory).
#   stage.sh DESTDIR PREFIX BINARY
set -eu
[ $# -eq 3 ] || { echo "usage: $0 DESTDIR PREFIX BINARY" >&2; exit 2; }
DEST=$1$2
BIN=$3
HERE=$(cd "$(dirname "$0")" && pwd)
TOP=$(cd "$HERE/.." && pwd)
[ "$(basename "$HERE")" = linux ] && [ -f "$TOP/README.md" ] || TOP=$(cd "$HERE/../.." && pwd)

install -Dm755 "$BIN" "$DEST/bin/cecilia"
install -Dm644 "$HERE/cecilia.desktop" "$DEST/share/applications/cecilia.desktop"
install -Dm644 "$HERE/cecilia.svg" "$DEST/share/icons/hicolor/scalable/apps/cecilia.svg"
install -Dm644 "$HERE/cecilia-mime.xml" "$DEST/share/mime/packages/cecilia.xml"
# AppStream metadata: software centres (GNOME Software, Ubuntu App Center) take the app's name,
# icon and developer from it.
install -Dm644 "$HERE/io.github.kjmcnamara1.cecilia.metainfo.xml" \
    "$DEST/share/metainfo/io.github.kjmcnamara1.cecilia.metainfo.xml"
install -Dm644 "$TOP/LICENSE-MIT" "$DEST/share/licenses/cecilia/LICENSE-MIT"
install -Dm644 "$TOP/LICENSE-APACHE" "$DEST/share/licenses/cecilia/LICENSE-APACHE"
install -Dm644 "$TOP/README.md" "$DEST/share/doc/cecilia/README.md"
install -Dm644 "$TOP/CHANGELOG.md" "$DEST/share/doc/cecilia/CHANGELOG.md"
