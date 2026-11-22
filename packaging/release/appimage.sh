#!/usr/bin/env bash
# The AppImage, from the Linux release archive in IN_DIR (as the release workflow downloads it),
# into dist/. Downloads linuxdeploy unless $LINUXDEPLOY names one. Set APPIMAGE_EXTRACT_AND_RUN=1
# where FUSE is missing (CI).
#   appimage.sh IN_DIR      # mise run release:appimage in
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 IN_DIR" >&2; exit 2; }
in=$1
v=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
name=cecilia-v$v-x86_64-unknown-linux-gnu
tar xzf "$in/$name.tar.gz" -C "$in"
if [ -z "${LINUXDEPLOY:-}" ]; then
    mkdir -p target
    curl -fsSL -o target/linuxdeploy \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
    chmod +x target/linuxdeploy
    LINUXDEPLOY=$PWD/target/linuxdeploy
fi
export LINUXDEPLOY
packaging/appimage/build.sh "$v" "$in/$name/cecilia" dist
