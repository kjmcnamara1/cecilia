#!/usr/bin/env bash
# cargo-binstall finds the release archives (names must match release.yml's Package step).
set -eu
cd "$(dirname "$0")/../.."
fail() { echo "FAIL: $*" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq not installed: skipped"; exit 0; }
b=$(cargo metadata --no-deps --format-version 1 \
    | jq -c '.packages[] | select(.name == "cecilia") | .metadata.binstall')
check() { [ "$(jq -r "$1" <<< "$b")" = "$2" ] || fail "$1 is not $2"; }
check '.["pkg-url"]' '{ repo }/releases/download/v{ version }/cecilia-v{ version }-{ target }.{ archive-format }'
check '.["bin-dir"]' 'cecilia-v{ version }-{ target }/{ bin }{ binary-ext }'
check '.["pkg-fmt"]' 'tgz'
check '.overrides["x86_64-pc-windows-msvc"]["pkg-fmt"]' 'zip'
echo "binstall: ok"
