#!/usr/bin/env bash
# The .deb and .rpm, from the binary release/build.sh built for TARGET, into dist/. cargo-deb and
# cargo-generate-rpm (installed by the mise task) package the tree packaging/linux/stage.sh lays
# out; see [package.metadata.deb] in crates/cecilia/Cargo.toml.
#   linux-packages.sh TARGET   # mise run release:linux-packages x86_64-unknown-linux-gnu
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 TARGET" >&2; exit 2; }
rm -rf target/stage
packaging/linux/stage.sh target/stage /usr "target/$1/release/cecilia"
mkdir -p dist
cargo deb -p cecilia --no-build --no-strip --output dist/
cargo generate-rpm -p crates/cecilia --output dist/
