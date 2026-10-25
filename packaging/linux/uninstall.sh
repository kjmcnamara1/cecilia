#!/bin/sh
# Removes what install.sh put in PREFIX (default ~/.local).
set -eu
PREFIX=${1:-$HOME/.local}
cargo uninstall --root "$PREFIX" muse 2>/dev/null || rm -f "$PREFIX/bin/muse"
rm -f "$PREFIX/share/applications/muse.desktop" \
      "$PREFIX/share/icons/hicolor/scalable/apps/muse.svg" \
      "$PREFIX/share/mime/packages/muse.xml"
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
echo "muse removed from $PREFIX"
