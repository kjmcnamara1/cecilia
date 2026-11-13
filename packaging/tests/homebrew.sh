#!/usr/bin/env bash
# packaging/homebrew/render.sh turns SHA256SUMS into a formula and a cask.
set -eu
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

V=1.2.3
a=$(printf 'a%.0s' $(seq 64)); b=$(printf 'b%.0s' $(seq 64))
c=$(printf 'c%.0s' $(seq 64)); d=$(printf 'd%.0s' $(seq 64))
cat > "$tmp/sums" <<SUMS
$a  cecilia-v$V-x86_64-unknown-linux-gnu.tar.gz
$b  cecilia-v$V-aarch64-apple-darwin.tar.gz
$c  cecilia-v$V-x86_64-apple-darwin.tar.gz
$d  cecilia-v$V-universal-apple-darwin.dmg
SUMS

sh packaging/homebrew/render.sh $V "$tmp/sums" "$tmp/tap" || fail "render failed"
for f in Formula/cecilia.rb Casks/cecilia.rb; do
    [ -f "$tmp/tap/$f" ] || fail "no $f"
    ruby -c "$tmp/tap/$f" >/dev/null || fail "$f is not valid Ruby"
    ! grep -q '@[A-Z_0-9]*@' "$tmp/tap/$f" || fail "$f has an unfilled placeholder"
done
formula=$tmp/tap/Formula/cecilia.rb
for pair in "x86_64-unknown-linux-gnu:$a" "aarch64-apple-darwin:$b" "x86_64-apple-darwin:$c"; do
    grep -q "cecilia-v$V-${pair%%:*}.tar.gz" "$formula" || fail "formula lacks the ${pair%%:*} url"
    grep -q "sha256 \"${pair#*:}\"" "$formula" || fail "formula lacks the ${pair%%:*} hash"
done
grep -q "sha256 \"$d\"" "$tmp/tap/Casks/cecilia.rb" || fail "cask lacks the dmg hash"
grep -q 'universal-apple-darwin.dmg' "$tmp/tap/Casks/cecilia.rb" || fail "cask lacks the dmg url"

# a missing checksum fails and writes nothing
grep -v '\.dmg$' "$tmp/sums" > "$tmp/short"
if sh packaging/homebrew/render.sh $V "$tmp/short" "$tmp/tap2" 2>"$tmp/err"; then
    fail "missing checksum accepted"
fi
grep -q 'universal-apple-darwin.dmg' "$tmp/err" || fail "error does not name the asset"
[ ! -e "$tmp/tap2/Casks/cecilia.rb" ] || fail "cask written without a checksum"
echo "homebrew: ok"
