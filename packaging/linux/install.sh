#!/bin/sh
# Installs muse with its desktop entry, icon and file types.
#   packaging/linux/install.sh            # into ~/.local (no root needed)
#   sudo packaging/linux/install.sh /usr/local
# PREFIX/bin must be on PATH for the desktop entry to find `muse`.
set -eu
PREFIX=${1:-$HOME/.local}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

if [ "${MUSE_SKIP_BUILD:-}" != 1 ]; then
    cargo install --locked --path "$ROOT/crates/muse" --root "$PREFIX"
fi
install -Dm644 "$HERE/muse.desktop" "$PREFIX/share/applications/muse.desktop"
install -Dm644 "$HERE/muse.svg" "$PREFIX/share/icons/hicolor/scalable/apps/muse.svg"
install -Dm644 "$HERE/muse-mime.xml" "$PREFIX/share/mime/packages/muse.xml"

# Refresh the desktop's caches (each is optional).
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
# Only refresh an icon cache that exists: a new, stale one would hide icons installed later.
if [ -f "$PREFIX/share/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi
echo "muse installed in $PREFIX"
