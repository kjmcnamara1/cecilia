#!/usr/bin/env bash
# Finishes the release assets in dist/ (the archives, packages and installers of every build
# job): SHA256SUMS and install.sh, and writes the release notes to notes.md (outside dist/, so
# they are not uploaded as an asset).
#   assets.sh             # mise run release:assets
set -euo pipefail
cd "$(dirname "$0")/../.."
v=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
# The archives, packages and cecilia-win-Setup.exe.
(cd dist && sha256sum cecilia* > SHA256SUMS)
# Not in SHA256SUMS: it is what reads them.
cp packaging/install.sh dist/install.sh

# The CHANGELOG section of this version, or [Unreleased] for a pre-release before it.
section() {
    awk -v h="## [$1]" 'index($0, h) == 1 { on = 1; next } /^## \[/ { on = 0 } on' CHANGELOG.md
}
section "$v" > notes.md
[ -n "$(tr -d '[:space:]' < notes.md)" ] || section Unreleased > notes.md
[ -s notes.md ] || echo "See CHANGELOG.md." > notes.md
cat >> notes.md <<'NOTE'

The binaries are not signed: Windows SmartScreen and macOS Gatekeeper will warn. On
macOS run `xattr -d com.apple.quarantine cecilia` after unpacking. Check downloads
against `SHA256SUMS`.
NOTE
