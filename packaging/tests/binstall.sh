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
# binstall needs every binary of the crate in the archive, so the Package step must copy them all.
bins=$(cargo metadata --no-deps --format-version 1 \
    | jq -r '[.packages[] | select(.name == "cecilia") | .targets[]
              | select(.kind | index("bin")) | .name] | sort | join(",")')
grep -qF "release/{$bins}\$exe \"\$name/\"" .github/workflows/release.yml \
    || fail "release.yml's Package step does not copy every binary ({$bins})"
echo "binstall: ok"
