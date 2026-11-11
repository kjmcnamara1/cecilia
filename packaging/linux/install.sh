#!/bin/sh
# Installs cecilia with its desktop entry, icon and file types.
#   packaging/linux/install.sh            # into ~/.local (no root needed)
#   sudo packaging/linux/install.sh /usr/local
# From a source checkout it builds cecilia first (needs a Rust toolchain); from a release
# tarball it installs the `cecilia` binary shipped next to this directory.
# PREFIX/bin must be on PATH for the desktop entry to find `cecilia`.
set -eu
PREFIX=${1:-$HOME/.local}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)

if [ -x "$ROOT/cecilia" ]; then
    BIN=$ROOT/cecilia
else
    ROOT=$(cd "$HERE/../.." && pwd)
    cargo build --release --locked -p cecilia --manifest-path "$ROOT/Cargo.toml"
    BIN=$ROOT/target/release/cecilia
fi
"$HERE/stage.sh" / "$PREFIX" "$BIN"

# Refresh the desktop's caches (each is optional).
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
# Only refresh an icon cache that exists: a new, stale one would hide icons installed later.
if [ -f "$PREFIX/share/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi
echo "cecilia installed in $PREFIX"
