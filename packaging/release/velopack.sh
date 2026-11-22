#!/usr/bin/env bash
# The Windows installer (Velopack), from the binaries release/build.sh built for TARGET. Needs the
# .NET SDK. Adds the installer, this version's packages and the feed to dist/.
#   velopack.sh TARGET   # mise run release:velopack x86_64-pc-windows-msvc
# The version is the tag's ($GITHUB_REF_NAME), not Cargo.toml's: a pre-release must sort below
# its final release in Velopack. Outside CI it is the workspace version. $GH_TOKEN, when set,
# authenticates the download of the latest release.
set -euo pipefail
cd "$(dirname "$0")/../.."
[ $# -eq 1 ] || { echo "usage: $0 TARGET" >&2; exit 2; }
target=$1
tag=${GITHUB_REF_NAME:-v$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)}
v=${tag#v}
repo=${GITHUB_REPOSITORY:-kjmcnamara1/cecilia}
work=target/velopack
rm -rf "$work"
mkdir -p "$work/pack" dist

# vpk must be the version of the velopack crate cecilia links (Cargo.lock).
vpk_version=$(awk '{ sub(/\r$/, "") } $0 == "name = \"velopack\"" { getline; gsub(/version = |"|\r/, ""); print; exit }' Cargo.lock)
[ -n "$vpk_version" ] || { echo "::error::no velopack in Cargo.lock" >&2; exit 1; }
dotnet tool install vpk --version "$vpk_version" --tool-path "$work/vpk"
vpk=$work/vpk/vpk

cp "target/$target"/release/{cecilia,ceciliag}.exe "$work/pack/"
cp LICENSE-APACHE LICENSE-MIT README.md CHANGELOG.md "$work/pack/"
# The latest release's feed and package, so this release gets a delta package and a feed listing
# both. There is none before the first release with an installer.
token=()
[ -z "${GH_TOKEN:-}" ] || token=(--token "$GH_TOKEN")
"$vpk" download github --repoUrl "https://github.com/$repo" "${token[@]}" \
    --outputDir "$work/Releases" || echo "no earlier installer release"
# Per-user install with a Start Menu shortcut to ceciliag.exe and an uninstall entry.
"$vpk" pack --packId cecilia --packVersion "$v" --packDir "$work/pack" \
    --mainExe ceciliag.exe --icon crates/cecilia/assets/cecilia.ico \
    --packTitle cecilia --packAuthors "The cecilia contributors" --runtime win-x64 \
    --shortcuts StartMenuRoot --noPortable --outputDir "$work/Releases"
ls -l "$work/Releases"
# Released with the other assets: the installer, this version's packages and the feed (installed
# copies read releases.win.json from the latest release).
cp "$work"/Releases/{cecilia-win-Setup.exe,releases.win.json} dist/
cp "$work/Releases/cecilia-$v-"*.nupkg dist/
