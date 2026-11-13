#!/usr/bin/env bash
# packaging/install.sh against a fake release served from a file:// URL (Linux paths; the
# macOS path is covered by packaging/macos/test.sh).
set -eu
cd "$(dirname "$0")/../.."
ROOT=$PWD
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

V=0.0.1
T=x86_64-unknown-linux-gnu
name=cecilia-v$V-$T
rel=$tmp/rel/download/v$V
mkdir -p "$rel" "$tmp/src/$name"
# the real archive layout
printf '#!/bin/sh\necho "cecilia %s"\n' "$V" > "$tmp/src/$name/cecilia"
chmod +x "$tmp/src/$name/cecilia"
cp LICENSE-APACHE LICENSE-MIT README.md CHANGELOG.md "$tmp/src/$name/"
cp -r packaging/linux "$tmp/src/$name/linux"
tar czf "$rel/$name.tar.gz" -C "$tmp/src" "$name"
(cd "$rel" && sha256sum "$name.tar.gz" > SHA256SUMS)
cp -r "$tmp/rel" "$tmp/rel-good"

# a uname that reports what UNAME_S / UNAME_M say
mkdir "$tmp/bin"
cat > "$tmp/bin/uname" <<'SH'
#!/bin/sh
case $1 in -s) echo "${UNAME_S:-Linux}" ;; -m) echo "${UNAME_M:-x86_64}" ;; *) echo "${UNAME_S:-Linux}" ;; esac
SH
chmod +x "$tmp/bin/uname"

run() { # run PREFIX [env...]: the installer, output in $tmp/out
    local prefix=$1; shift
    env PATH="$tmp/bin:$PATH" HOME="$tmp/home" CECILIA_BASE_URL="file://$tmp/rel" \
        CECILIA_VERSION=$V CECILIA_PREFIX="$prefix" "$@" sh "$ROOT/packaging/install.sh" \
        > "$tmp/out" 2>&1
}

# 1. Linux x86_64 installs and verifies
run "$tmp/p1" || { cat "$tmp/out"; fail "install failed"; }
[ "$("$tmp/p1/bin/cecilia" --version)" = "cecilia $V" ] || fail "binary not installed"
[ -f "$tmp/p1/share/applications/cecilia.desktop" ] || fail "no desktop entry"

# 2. a tampered download is refused and nothing is installed
printf x >> "$rel/$name.tar.gz"
if run "$tmp/p2"; then fail "tampered archive accepted"; fi
grep -qi checksum "$tmp/out" || fail "no checksum message: $(cat "$tmp/out")"
[ ! -e "$tmp/p2" ] || fail "tampered archive left files behind"

# 3. SHA256SUMS without the asset's line
cp "$tmp/rel-good/download/v$V/$name.tar.gz" "$rel/$name.tar.gz"
: > "$rel/SHA256SUMS"
if run "$tmp/p3"; then fail "missing checksum accepted"; fi
[ ! -e "$tmp/p3" ] || fail "missing checksum left files behind"

# 4. unsupported platforms: no download, pointer to cargo
rm -rf "$tmp/rel"; mkdir "$tmp/rel"
for plat in "Linux aarch64" "FreeBSD amd64"; do
    read -r s m <<< "$plat"
    if run "$tmp/p4" UNAME_S="$s" UNAME_M="$m"; then fail "$plat accepted"; fi
    grep -q 'cargo install' "$tmp/out" || fail "$plat: no cargo hint: $(cat "$tmp/out")"
done
echo "install: ok"
