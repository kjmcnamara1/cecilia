#!/bin/sh
# Installs the latest cecilia release:
#   curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh | sh
# Linux (x86_64): the release tarball, installed like packaging/linux/install.sh does (binary,
#   desktop entry, icon, file types) under $CECILIA_PREFIX (default ~/.local).
# macOS: Cecilia.app from the release .dmg into $CECILIA_APP_DIR (default ~/Applications), and
#   a `cecilia` command in $CECILIA_BIN_DIR (default ~/.local/bin).
# Downloads are checked against the release's SHA256SUMS. Other variables:
#   CECILIA_VERSION   a version such as 0.46.0 (default: the latest release)
#   CECILIA_BASE_URL  default https://github.com/kjmcnamara1/cecilia/releases
#   CECILIA_NO_MODIFY_PATH=1  don't add the bin directory to PATH in the shell's startup file
set -eu

BASE=${CECILIA_BASE_URL:-https://github.com/kjmcnamara1/cecilia/releases}

die() {
    echo "install.sh: $*" >&2
    exit 1
}

# Echoes the asset's checksum from SHA256SUMS ($1 = sums file, $2 = asset name).
expected_sum() {
    grep " $2\$" "$1" | cut -d' ' -f1 | head -1
}

actual_sum() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# Puts $1 on PATH for new shells unless it is there already: a line in the startup file of the
# user's shell ($SHELL), added once. CECILIA_NO_MODIFY_PATH=1 only prints a note.
on_path() {
    case ":$PATH:" in *":$1:"*) return ;; esac
    if [ "${CECILIA_NO_MODIFY_PATH:-0}" = 1 ]; then
        echo "note: add $1 to your PATH"
        return
    fi
    case "${SHELL##*/}" in
        zsh) rc=${ZDOTDIR:-$HOME}/.zshrc ;;
        bash)
            # Terminal.app starts login shells, which read .bash_profile and not .bashrc
            if [ "$(uname -s)" = Darwin ]; then rc=$HOME/.bash_profile; else rc=$HOME/.bashrc; fi
            ;;
        fish) rc=${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/cecilia.fish ;;
        *) rc=$HOME/.profile ;;
    esac
    if [ -f "$rc" ] && grep -qF "$1" "$rc"; then
        return
    fi
    mkdir -p "$(dirname "$rc")"
    if [ "${SHELL##*/}" = fish ]; then
        line="contains -- \"$1\" \$PATH; or set -gx PATH \"$1\" \$PATH"
    else
        line="export PATH=\"$1:\$PATH\""
    fi
    printf '\n# added by the cecilia installer\n%s\n' "$line" >> "$rc"
    echo "added $1 to your PATH in $rc; open a new terminal to use it"
}

main() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os $arch" in
        "Linux x86_64") kind=linux ;;
        "Darwin arm64" | "Darwin x86_64") kind=macos ;;
        *)
            die "no prebuilt cecilia for $os $arch; build it with: cargo install --locked cecilia (or cargo binstall cecilia)"
            ;;
    esac

    version=${CECILIA_VERSION:-}
    if [ -z "$version" ]; then
        tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$BASE/latest") \
            || die "cannot find the latest release at $BASE/latest"
        version=${tag##*/v}
    fi
    case "$kind" in
        linux) asset=cecilia-v$version-x86_64-unknown-linux-gnu.tar.gz ;;
        macos) asset=cecilia-v$version-universal-apple-darwin.dmg ;;
    esac

    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT INT TERM
    echo "downloading cecilia $version ($asset)"
    curl -fsSL -o "$tmp/$asset" "$BASE/download/v$version/$asset" || die "cannot download $asset"
    curl -fsSL -o "$tmp/SHA256SUMS" "$BASE/download/v$version/SHA256SUMS" \
        || die "cannot download SHA256SUMS"
    want=$(expected_sum "$tmp/SHA256SUMS" "$asset")
    [ -n "$want" ] || die "SHA256SUMS has no checksum for $asset"
    [ "$(actual_sum "$tmp/$asset")" = "$want" ] || die "checksum mismatch for $asset; nothing installed"

    case "$kind" in
        linux)
            prefix=${CECILIA_PREFIX:-$HOME/.local}
            tar xzf "$tmp/$asset" -C "$tmp"
            "$tmp/cecilia-v$version-x86_64-unknown-linux-gnu/linux/install.sh" "$prefix"
            on_path "$prefix/bin"
            ;;
        macos)
            app_dir=${CECILIA_APP_DIR:-$HOME/Applications}
            bin_dir=${CECILIA_BIN_DIR:-$HOME/.local/bin}
            mkdir -p "$tmp/mnt" "$app_dir" "$bin_dir"
            hdiutil attach -nobrowse -readonly -mountpoint "$tmp/mnt" "$tmp/$asset" >/dev/null
            rm -rf "$app_dir/Cecilia.app"
            cp -R "$tmp/mnt/Cecilia.app" "$app_dir/Cecilia.app"
            hdiutil detach -quiet "$tmp/mnt" || true
            xattr -dr com.apple.quarantine "$app_dir/Cecilia.app" 2>/dev/null || true
            ln -sf "$app_dir/Cecilia.app/Contents/MacOS/cecilia" "$bin_dir/cecilia"
            echo "Cecilia.app installed in $app_dir, command: $bin_dir/cecilia"
            on_path "$bin_dir"
            ;;
    esac
}

main "$@"
