#!/usr/bin/env bash
# The release workflow's first check: the tag (vX.Y.Z, or vX.Y.Z-rc.N for a pre-release) must
# match the workspace version in Cargo.toml. Prints `version=` and `prerelease=` lines, and in
# GitHub Actions appends them to $GITHUB_OUTPUT.
#   meta.sh TAG          # mise run release:meta v0.50.0
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 TAG" >&2; exit 2; }
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
tag=${1#v}
if [ "${tag%%-*}" != "$version" ]; then
    echo "::error::tag $1 does not match the workspace version $version" >&2
    exit 1
fi
case "$tag" in *-*) pre=true ;; *) pre=false ;; esac
out=$(printf 'version=%s\nprerelease=%s\n' "$version" "$pre")
echo "$out"
if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "$out" >> "$GITHUB_OUTPUT"; fi
