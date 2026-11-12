# https://just.systems/man/en
set shell := ["bash", "-euo", "pipefail", "-c"]

# Set the release version everywhere: `just bump major|minor|patch|X.Y.Z`. Edits the working
# tree only (Cargo.toml and Cargo.lock, CHANGELOG.md, the AUR PKGBUILDs and the Scoop, Winget
# and Chocolatey manifests); committing and tagging are left to the caller. The packaging
# checksums are filled in after the release is published (the release workflow's AUR job
# does the PKGBUILDs, packaging/bump-windows.sh the Windows manifests by hand).
bump version:
    #!/usr/bin/env bash
    set -euo pipefail
    cur=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
    IFS=. read -r major minor patch <<< "$cur"
    case "{{version}}" in
      major) new=$((major + 1)).0.0 ;;
      minor) new=$major.$((minor + 1)).0 ;;
      patch) new=$major.$minor.$((patch + 1)) ;;
      *) new={{version}} ;;
    esac
    [[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
      || { echo "not major, minor, patch or X.Y.Z: {{version}}" >&2; exit 1; }
    [ "$new" != "$cur" ] || { echo "already at $cur" >&2; exit 1; }
    awk '/^## \[Unreleased\]/ { on = 1; next } /^## \[/ { on = 0 } on' CHANGELOG.md \
      | grep -q '[^[:space:]]' || { echo "CHANGELOG.md has nothing under [Unreleased]" >&2; exit 1; }

    # Every packaging file must be at the current version, or it has drifted: stop before editing.
    win=(packaging/scoop/cecilia.json packaging/winget/*.yaml packaging/choco/cecilia.nuspec
      packaging/choco/tools/chocolateyinstall.ps1)
    for f in "${win[@]}" packaging/aur/cecilia{,-bin}/PKGBUILD; do
      grep -qF "$cur" "$f" || { echo "$f is not at $cur" >&2; exit 1; }
    done

    old=${cur//./\\.}
    sed -i -e "s/^version = \"$old\"/version = \"$new\"/" \
      -e "s|\(path = \"crates/[^\"]*\", version = \"\)$old\"|\1$new\"|" Cargo.toml
    cargo update --workspace --offline
    sed -i "s/^## \[Unreleased\]/&\n\n## [$new] - $(date +%F)/" CHANGELOG.md

    sed -i -e "s/^pkgver=.*/pkgver=$new/" -e "s/^pkgrel=.*/pkgrel=1/" packaging/aur/cecilia{,-bin}/PKGBUILD
    sed -i "s/$old/$new/g" "${win[@]}"
    echo "$cur -> $new"
