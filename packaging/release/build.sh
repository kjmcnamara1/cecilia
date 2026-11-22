#!/usr/bin/env bash
# Builds the release binaries for TARGET, runs them when TARGET is the host (the Intel macOS
# binary is cross-built on an Apple silicon runner) and packs them with the licences, README and
# CHANGELOG (and packaging/linux on Linux) into dist/cecilia-vVERSION-TARGET.tar.gz, or .zip on
# Windows: the archives cargo-binstall downloads (packaging/tests/binstall.sh).
#   build.sh TARGET      # mise run release:build x86_64-unknown-linux-gnu
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 TARGET" >&2; exit 2; }
target=$1
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
exe=
case "$target" in *windows*) exe=.exe ;; esac

cargo build --release --locked -p cecilia --target "$target"
if [ "$target" = "$(rustc -vV | sed -n 's/^host: //p')" ]; then
    "target/$target/release/cecilia$exe" --version
    # ceciliag.exe has no console to print to: only check it was built.
    [ -z "$exe" ] || test -f "target/$target/release/ceciliag.exe"
fi

name=cecilia-v$version-$target
stage=target/package
rm -rf "${stage:?}/$name"
mkdir -p "$stage/$name" dist
# Every binary of the crate: cargo-binstall refuses an archive that lacks one.
# ceciliag(.exe) is `cecilia --gui`; on Windows without a console (Start Menu shortcuts).
cp "target/$target"/release/{cecilia,ceciliag}"$exe" "$stage/$name/"
cp LICENSE-APACHE LICENSE-MIT README.md CHANGELOG.md "$stage/$name/"
case "$target" in *linux*) cp -r packaging/linux "$stage/$name/" ;; esac
if [ -n "$exe" ]; then
    (cd "$stage" && 7z a "$OLDPWD/dist/$name.zip" "$name")
else
    tar czf "dist/$name.tar.gz" -C "$stage" "$name"
fi
ls -l dist
