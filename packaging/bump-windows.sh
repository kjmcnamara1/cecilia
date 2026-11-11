#!/bin/sh
# Points the Scoop, Winget and Chocolatey manifests at a published release:
#   packaging/bump-windows.sh 0.45.0
# Run after the release workflow has published v0.45.0. It reads the zip's checksum from the
# release's SHA256SUMS. Then test the manifests on Windows (see the plan) and submit them.
set -eu
NEW=${1:?usage: $0 VERSION}
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=kjmcnamara1/cecilia
ZIP=cecilia-v$NEW-x86_64-pc-windows-msvc.zip

OLD=$(sed -n 's/^ *"version": "\(.*\)",/\1/p' "$HERE/scoop/cecilia.json" | head -1)
OLD_HASH=$(sed -n 's/^ *"hash": "\([0-9a-f]\{64\}\)",/\1/p' "$HERE/scoop/cecilia.json" | head -1)
HASH=$(curl -fsSL "https://github.com/$REPO/releases/download/v$NEW/SHA256SUMS" | grep " $ZIP\$" | cut -d' ' -f1)
[ -n "$HASH" ] || { echo "no $ZIP in SHA256SUMS" >&2; exit 1; }

FILES="$HERE/scoop/cecilia.json $HERE/winget/*.yaml $HERE/choco/cecilia.nuspec $HERE/choco/tools/chocolateyinstall.ps1"
# shellcheck disable=SC2086
sed -i -e "s/$OLD/$NEW/g" \
    -e "s/$OLD_HASH/$HASH/g" \
    -e "s/$(echo "$OLD_HASH" | tr a-f A-F)/$(echo "$HASH" | tr a-f A-F)/g" $FILES
echo "manifests set to $NEW ($HASH)"
