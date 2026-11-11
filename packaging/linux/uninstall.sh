#!/bin/sh
# Removes what install.sh put in PREFIX (default ~/.local).
set -eu
PREFIX=${1:-$HOME/.local}
# An older install.sh used `cargo install`; this removes that too.
cargo uninstall --root "$PREFIX" cecilia 2>/dev/null || true
rm -f "$PREFIX/bin/cecilia" \
      "$PREFIX/share/applications/cecilia.desktop" \
      "$PREFIX/share/icons/hicolor/scalable/apps/cecilia.svg" \
      "$PREFIX/share/mime/packages/cecilia.xml"
rm -rf "$PREFIX/share/licenses/cecilia" "$PREFIX/share/doc/cecilia"
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
echo "cecilia removed from $PREFIX"
