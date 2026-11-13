#!/usr/bin/env bash
# Every shell script under packaging/ is shellcheck-clean (skipped when shellcheck is missing).
set -eu
cd "$(dirname "$0")/../.."
command -v shellcheck >/dev/null || { echo "shellcheck not installed: skipped"; exit 0; }
shellcheck packaging/check.sh packaging/tests/*.sh packaging/*/*.sh packaging/macos/cecilia-gui \
    packaging/linux/*.sh
echo "shellcheck: ok"
