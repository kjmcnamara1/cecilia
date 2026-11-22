#!/usr/bin/env bash
# The universal macOS app: the aarch64 and x86_64 binaries from the release archives in
# IN_DIR/<target>/ (as the release workflow downloads them) joined with lipo, wrapped in
# Cecilia.app and packed into dist/cecilia-vVERSION-universal-apple-darwin.dmg. macOS only;
# packaging/macos/bundle.sh lists what it needs.
#   macos-app.sh IN_DIR     # mise run release:macos-app in
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 IN_DIR" >&2; exit 2; }
in=$1
v=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
bins=()
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    tar xzf "$in/$t/cecilia-v$v-$t.tar.gz" -C "$in/$t"
    bins+=("$in/$t/cecilia-v$v-$t/cecilia")
done
mkdir -p dist target
lipo -create -output target/cecilia-universal "${bins[@]}"
packaging/macos/bundle.sh "$v" target/cecilia-universal "dist/cecilia-v$v-universal-apple-darwin.dmg"
