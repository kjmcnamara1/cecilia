#!/usr/bin/env bash
# Runs every packaging test (packaging/tests/*.sh); each exits non-zero on failure.
#   bash packaging/check.sh [NAME...]     # e.g. `bash packaging/check.sh homebrew`
set -eu
cd "$(dirname "$0")/.."
if [ $# -gt 0 ]; then
    tests=(); for n in "$@"; do tests+=("packaging/tests/$n.sh"); done
else
    tests=(packaging/tests/*.sh)
fi
for t in "${tests[@]}"; do
    echo "== $t"
    bash "$t"
done
echo "packaging checks passed"
