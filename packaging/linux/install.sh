#!/bin/sh
# Installs cecilia with its desktop entry, icon and file types.
#   packaging/linux/install.sh            # into ~/.local (no root needed)
#   sudo packaging/linux/install.sh /usr/local
# PREFIX/bin must be on PATH for the desktop entry to find `cecilia`.
set -eu
PREFIX=${1:-$HOME/.local}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

if [ "${CECILIA_SKIP_BUILD:-}" != 1 ]; then
    cargo install --locked --path "$ROOT/crates/cecilia" --root "$PREFIX"
fi
install -Dm644 "$HERE/cecilia.desktop" "$PREFIX/share/applications/cecilia.desktop"
install -Dm644 "$HERE/cecilia.svg" "$PREFIX/share/icons/hicolor/scalable/apps/cecilia.svg"
install -Dm644 "$HERE/cecilia-mime.xml" "$PREFIX/share/mime/packages/cecilia.xml"

# Refresh the desktop's caches (each is optional).
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
# Only refresh an icon cache that exists: a new, stale one would hide icons installed later.
if [ -f "$PREFIX/share/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi
echo "cecilia installed in $PREFIX"
