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
    env PATH="$tmp/bin:$PATH" HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/home/.config" ZDOTDIR="$tmp/home" \
        SHELL=/bin/bash CECILIA_BASE_URL="file://$tmp/rel" \
        CECILIA_VERSION=$V CECILIA_PREFIX="$prefix" "$@" sh "$ROOT/packaging/install.sh" \
        > "$tmp/out" 2>&1
}

# 1. Linux x86_64 installs and verifies
run "$tmp/p1" || { cat "$tmp/out"; fail "install failed"; }
[ "$("$tmp/p1/bin/cecilia" --version)" = "cecilia $V" ] || fail "binary not installed"
[ -f "$tmp/p1/share/applications/cecilia.desktop" ] || fail "no desktop entry"

# 1b. the bin directory is put on PATH once, in the startup file of the user's shell
rc=$tmp/home/.bashrc
grep -qxF "export PATH=\"$tmp/p1/bin:\$PATH\"" "$rc" || fail "bash: PATH not added: $(cat "$rc")"
run "$tmp/p1" || fail "reinstall failed"
[ "$(grep -c "$tmp/p1/bin" "$rc")" = 1 ] || fail "PATH line added twice"
run "$tmp/p1" SHELL=/bin/zsh || fail "zsh install failed"
grep -qF "$tmp/p1/bin" "$tmp/home/.zshrc" || fail "zsh: PATH not added"
run "$tmp/p1" SHELL=/usr/bin/fish || fail "fish install failed"
grep -qF "set -gx PATH \"$tmp/p1/bin\"" "$tmp/home/.config/fish/conf.d/cecilia.fish" \
    || fail "fish: PATH not added"
run "$tmp/p1" SHELL=/bin/dash || fail "sh install failed"
grep -qF "$tmp/p1/bin" "$tmp/home/.profile" || fail "sh: PATH not added"
rm -rf "${tmp:?}/home"
run "$tmp/p1" CECILIA_NO_MODIFY_PATH=1 || fail "opt-out install failed"
[ ! -e "$rc" ] || fail "CECILIA_NO_MODIFY_PATH=1 still wrote $rc"
grep -q "add $tmp/p1/bin to your PATH" "$tmp/out" || fail "opt-out: no note"
run "$tmp/p1" PATH="$tmp/p1/bin:$tmp/bin:$PATH" || fail "install with bin on PATH failed"
[ ! -e "$rc" ] || fail "wrote $rc although the bin directory was on PATH"

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
