# Easy Install Implementation Plan

> **Status: implemented on branch feat/easy-install (2026-11-13); awaiting the user's review. Only CI on macOS and the first `-rc` tag can run the macOS bundle, AppImage, Flatpak and Snap jobs.**
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make cecilia installable without a Rust toolchain on macOS (`.app` + `.dmg`, Homebrew), Linux (AppImage, Flatpak, Snap, existing deb/rpm/AUR) and anywhere with `curl` (an install script), plus `cargo binstall`.

**Architecture:** The release workflow (`.github/workflows/release.yml`) already builds per-target tarballs. New release jobs turn those into a universal `.dmg`, an AppImage, a Flatpak bundle and a Snap; a `homebrew` job renders a formula and a cask into a separate tap repo; `packaging/install.sh` (also a release asset) installs from the release. Every package reuses `packaging/linux/stage.sh` and its desktop entry, icon and mime files instead of duplicating them. Every script has a shell test under `packaging/tests/` that runs in CI and in `just ci`.

**Tech Stack:** POSIX sh, bash tests, GitHub Actions, `rsvg-convert` + `iconutil` + `hdiutil` + `lipo` (macOS), linuxdeploy (AppImage), flatpak-builder, snapcraft, Ruby (`ruby -c` for the Homebrew files).

**Spec:** the user's request in the 2026-11-13 conversation (no spec file). Binding points: the `.app` launches the GUI; its icon comes from `crates/cecilia-gui/assets/lily.svg`; channels wanted are Homebrew, Snap, Flatpak, AppImage, a curl install script, cargo-binstall; not wanted: COPR/OBS/PPA, MSI/MSIX, MacPorts; all documentation updated; the PR is reviewed by the user before merge (do not merge).

## Global Constraints

- Conventional Commits, branch `feat/easy-install`, never merge to `main`. Commit trailer: `Co-Authored-By: Claude Code <noreply@anthropic.com>`.
- Never edit versions by hand; `just bump` owns them. Anything versioned that this plan adds must be handled by `just bump` or be rendered at release time (no committed version strings).
- Release asset names (all start with `cecilia` so `sha256sum cecilia*` covers them, except `install.sh`):
  - `cecilia-vX.Y.Z-universal-apple-darwin.dmg` (contains `Cecilia.app` and an `Applications` symlink)
  - `cecilia-vX.Y.Z-x86_64.AppImage`
  - `cecilia-vX.Y.Z.flatpak`
  - `cecilia_X.Y.Z_amd64.snap` (name chosen by snapcraft; attach `cecilia_*.snap`)
  - `install.sh`
- App ID (Flatpak): `io.github.kjmcnamara1.cecilia`. Snap name: `cecilia`. Tap repo: `kjmcnamara1/homebrew-cecilia`.
- The macOS bundle's executable is `Contents/MacOS/cecilia-gui` (a shell launcher) next to the real `cecilia`: macOS file systems are case-insensitive, so the launcher must not be called `Cecilia`.
- Unsigned: macOS gets an ad-hoc `codesign -s -`, no notarization (needs a paid Apple account; out of scope, documented instead). No new Rust dependencies.
- Out of scope: Nix flake (no `nix` here to test it; a TODO entry instead), Windows installer script, auto-update, screenshots for Flathub (TODO entry), Finder "open with" events (winit does not deliver them; files open from the terminal or inside the app).
- New release jobs for AppImage, Flatpak and Snap use job-level `continue-on-error: true` (they are built blind until the first `-rc` tag and must not block a release); the macOS bundle job is strict because the installer and cask depend on it.

## Review Focus

- `install.sh` with a tampered download or missing `SHA256SUMS` entry must fail and install nothing (tested in Task 2).
- `install.sh` on an unsupported OS/arch (Linux aarch64, FreeBSD) must say so and point at `cargo install` / `cargo binstall`, not 404 (Task 2).
- The `.app` launcher must drop the `-psn_*` argument old macOS versions pass, and quote file names with spaces (Task 1).
- `just bump` must keep the Flatpak metainfo's release list in step, or Flathub validation rejects the next release (Task 5).
- Homebrew `render.sh` must fail loudly when a checksum is missing instead of writing an empty `sha256` (Task 3).

---

### Task 1: macOS `.app` and `.dmg`

**Files:**
- Create: `packaging/macos/Info.plist.in`, `packaging/macos/cecilia-gui`, `packaging/macos/make-iconset.sh`, `packaging/macos/bundle.sh`
- Create: `packaging/check.sh`, `packaging/tests/macos.sh`
- Modify: `justfile` (`ci` runs `packaging/check.sh`), `.github/workflows/ci.yml` (jobs `packaging` on ubuntu and `macos-bundle` on macOS), `.github/workflows/release.yml` (job `macos-app`, `release` needs it)

**Interfaces:**
- Produces: `packaging/macos/bundle.sh VERSION BINARY OUT_DMG` builds `Cecilia.app` around `BINARY` and writes `OUT_DMG` (needs `rsvg-convert`, `iconutil`, `hdiutil`, `codesign`, i.e. macOS). `packaging/macos/make-iconset.sh SVG OUT.iconset` writes the ten PNGs of an iconset using only `rsvg-convert`. `packaging/check.sh` runs every `packaging/tests/*.sh` (each exits non-zero on failure) and is the one entry point later tasks add tests to.
- `Info.plist.in` placeholders: `@VERSION@`.

- [ ] **Step 1: Write `packaging/tests/macos.sh` (Linux-runnable parts) and `packaging/check.sh`.** `check.sh` is `set -eu`, runs each `packaging/tests/*.sh` with `bash`, prints its name, and fails on the first failure. `macos.sh` asserts: (a) `make-iconset.sh crates/cecilia-gui/assets/lily.svg $tmp/i.iconset` produces `icon_16x16.png`, `icon_16x16@2x.png`, `icon_32x32.png`, `icon_32x32@2x.png`, `icon_128x128.png`, `icon_128x128@2x.png`, `icon_256x256.png`, `icon_256x256@2x.png`, `icon_512x512.png`, `icon_512x512@2x.png`, and `file` reports `1024 x 1024` for the last; (b) `xmllint --noout packaging/macos/Info.plist.in` passes and the plist names `CFBundleExecutable` `cecilia-gui`, contains `@VERSION@`, `NSMicrophoneUsageDescription`, and extensions `gp3 gp4 gp5 gp gpx gabc ccc ccm mid musicxml`; (c) launcher: copy `cecilia-gui` beside a stub `cecilia` that prints its arguments one per line; `cecilia-gui -psn_0_12345 "my song.gp5"` must print exactly `--gui` then `my song.gp5`; with no arguments it prints just `--gui`.
- [ ] **Step 2: Run `bash packaging/check.sh`.** Expected: FAIL (files missing).
- [ ] **Step 3: Implement the three files.** `make-iconset.sh`: loop over the five sizes, `rsvg-convert -w N -h N` for 1x and 2x names. `cecilia-gui`: POSIX sh; rotate the arguments through `set --` dropping `-psn_*`, then `exec "$(dirname "$0")/cecilia" --gui "$@"`. `Info.plist.in`: `CFBundleName`/`CFBundleDisplayName` `Cecilia`, `CFBundleIdentifier` `io.github.kjmcnamara1.cecilia`, `CFBundleExecutable` `cecilia-gui`, `CFBundleIconFile` `cecilia`, `CFBundleShortVersionString` and `CFBundleVersion` `@VERSION@`, `LSMinimumSystemVersion` `11.0`, `NSHighResolutionCapable` true, `LSApplicationCategoryType` `public.app-category.music`, `NSMicrophoneUsageDescription` "cecilia listens to the microphone for the tuner.", and one `CFBundleDocumentTypes` entry (role Editor) listing the extensions above.
- [ ] **Step 4: Implement `bundle.sh`.** Stage `$tmp/Cecilia.app/Contents/{MacOS,Resources}`; copy `BINARY` to `MacOS/cecilia` and `cecilia-gui` beside it (chmod 755); `sed s/@VERSION@/$VERSION/` the plist into `Contents/Info.plist`; `make-iconset.sh` then `iconutil -c icns -o Resources/cecilia.icns`; `codesign --force --deep -s - Cecilia.app`; add `ln -s /Applications` to the stage dir; `hdiutil create -volname Cecilia -srcfolder $stage -format UDZO -ov OUT_DMG`.
- [ ] **Step 5: Run `bash packaging/check.sh`.** Expected: PASS. Also run `shellcheck packaging/macos/*.sh packaging/check.sh packaging/tests/*.sh`: clean.
- [ ] **Step 6: Wire CI and release.** `justfile` `ci`: add `bash packaging/check.sh` after the cargo checks. `ci.yml`: job `packaging` (ubuntu: `sudo apt-get install -y librsvg2-bin shellcheck desktop-file-utils appstream libxml2-utils`, then `bash packaging/check.sh`); job `macos-bundle` (macos-latest: `brew install librsvg`; make a stub binary that prints its arguments; `packaging/macos/bundle.sh 0.0.0 stub $RUNNER_TEMP/t.dmg`; `plutil -lint` the plist inside the mounted dmg; assert `Cecilia.app/Contents/Resources/cecilia.icns` exists and `Cecilia.app/Contents/MacOS/cecilia-gui x` prints `--gui` and `x`). `release.yml`: job `macos-app` (`needs: [meta, build]`, macos-latest): download artifacts `aarch64-apple-darwin` and `x86_64-apple-darwin` into separate dirs, untar each, `lipo -create -output cecilia` of the two binaries, `brew install librsvg`, `bundle.sh ${{ needs.meta.outputs.version }} cecilia dist/cecilia-v${version}-universal-apple-darwin.dmg`, upload artifact `macos-app` (`dist/*.dmg`); add `macos-app` to the `release` job's `needs`.
- [ ] **Step 7: Commit** `feat(packaging): macOS .app bundle and .dmg built by the release workflow`.

### Task 2: curl install script

**Files:**
- Create: `packaging/install.sh`, `packaging/tests/install.sh`, `packaging/macos/test.sh`
- Modify: `.github/workflows/release.yml` (copy `packaging/install.sh` into `dist/install.sh` in the `release` job before publishing, after the checksum step so it is not in `SHA256SUMS`), `.github/workflows/ci.yml` (`macos-bundle` runs `packaging/macos/test.sh`)

**Interfaces:**
- Consumes: asset names from Global Constraints; the release tarball layout (`cecilia-vX-TARGET/{cecilia,linux/install.sh,...}`); Task 1's `bundle.sh`.
- Produces: `packaging/install.sh`, used as `curl -fsSL https://github.com/kjmcnamara1/cecilia/releases/latest/download/install.sh | sh`. Environment: `CECILIA_VERSION` (default: latest, resolved from the `releases/latest` redirect), `CECILIA_PREFIX` (Linux, default `$HOME/.local`), `CECILIA_APP_DIR` (macOS, default `$HOME/Applications`), `CECILIA_BIN_DIR` (macOS, default `$HOME/.local/bin`), `CECILIA_BASE_URL` (default `https://github.com/kjmcnamara1/cecilia/releases`; assets live at `$BASE/download/vX.Y.Z/ASSET`).

- [ ] **Step 1: Write `packaging/tests/install.sh`.** It builds a fake release in a temp dir: a tarball `cecilia-v0.0.1-x86_64-unknown-linux-gnu.tar.gz` made from a real archive layout (stub `cecilia` binary that prints `cecilia 0.0.1`, real `packaging/linux/` copied as `linux/`, `LICENSE-*`, `README.md`, `CHANGELOG.md`) and a `SHA256SUMS`, under `$tmp/rel/download/v0.0.1/`. It runs the installer with `CECILIA_BASE_URL=file://$tmp/rel CECILIA_VERSION=0.0.1 CECILIA_PREFIX=$tmp/prefix HOME=$tmp/home` and a shim `uname` on `PATH`. Cases: (1) Linux x86_64: exit 0, `$prefix/bin/cecilia --version` works, `$prefix/share/applications/cecilia.desktop` exists; (2) tampered tarball (append a byte): exit non-zero, message mentions the checksum, `$prefix` stays empty; (3) `SHA256SUMS` without the asset's line: exit non-zero, nothing installed; (4) `uname -m` = `aarch64` on Linux and `uname -s` = `FreeBSD`: exit non-zero, message contains `cargo install` and no download attempted (empty fake release).
- [ ] **Step 2: Run `bash packaging/tests/install.sh`.** Expected: FAIL (script missing).
- [ ] **Step 3: Implement `packaging/install.sh`** (POSIX sh, `set -eu`, `main` called at the bottom so a truncated pipe runs nothing). Steps: map `uname -s`/`uname -m` to Linux `x86_64` → `x86_64-unknown-linux-gnu` tarball, Darwin (`arm64` or `x86_64`) → universal dmg; anything else → error naming the platform and suggesting `cargo install --locked cecilia` / `cargo binstall cecilia`; resolve version (`curl -fsSLI -o /dev/null -w '%{url_effective}' $BASE/latest`, strip up to `/v`); download asset and `SHA256SUMS` into a `mktemp -d` removed by a `trap`; verify with `sha256sum` or `shasum -a 256` (fail if the sums file has no line for the asset or the hash differs); Linux: extract, run the archive's `linux/install.sh "$PREFIX"`, warn if `$PREFIX/bin` is not on `PATH`; macOS: `hdiutil attach -nobrowse -readonly -mountpoint`, replace `$APP_DIR/Cecilia.app` with `cp -R`, `hdiutil detach`, `xattr -dr com.apple.quarantine`, symlink `$BIN_DIR/cecilia` to `Cecilia.app/Contents/MacOS/cecilia`, warn if `$BIN_DIR` is not on `PATH`. Print what was installed and where.
- [ ] **Step 4: Run `bash packaging/tests/install.sh` and `shellcheck packaging/install.sh`.** Expected: PASS, clean.
- [ ] **Step 5: Write `packaging/macos/test.sh`** (macOS only, run by the `macos-bundle` CI job): stub binary → `bundle.sh` → dmg; fake release dir with that dmg named `cecilia-v0.0.1-universal-apple-darwin.dmg` and a `SHA256SUMS` (`shasum -a 256`); run `install.sh` with the `file://` base, `HOME`, `CECILIA_APP_DIR` and `CECILIA_BIN_DIR` under a temp dir; assert `Cecilia.app/Contents/MacOS/cecilia-gui` exists and `$bin/cecilia` (symlink) runs. Replace the ad-hoc assertions of Task 1's CI job with a call to this script (it covers them: plist lint, icns, launcher). It cannot run here: ledger it as unverified until CI runs it.
- [ ] **Step 6: Release asset.** In `release.yml`'s `release` job, `cp packaging/install.sh dist/install.sh` after the `Checksums` step.
- [ ] **Step 7: Commit** `feat(packaging): curl install script`.

### Task 3: Homebrew tap (formula and cask)

**Files:**
- Create: `packaging/homebrew/cecilia.rb.in`, `packaging/homebrew/cecilia.cask.rb.in`, `packaging/homebrew/render.sh`, `packaging/tests/homebrew.sh`
- Modify: `.github/workflows/release.yml` (job `homebrew`)

**Interfaces:**
- Produces: `packaging/homebrew/render.sh VERSION SHA256SUMS TAP_DIR` writes `TAP_DIR/Formula/cecilia.rb` and `TAP_DIR/Casks/cecilia.rb`, replacing `@VERSION@`, `@SHA_LINUX_X86_64@`, `@SHA_DARWIN_ARM64@`, `@SHA_DARWIN_X86_64@`, `@SHA_DMG@` with the checksum of the matching asset from `SHA256SUMS`; it exits non-zero when an asset has no line.

- [ ] **Step 1: Write `packaging/tests/homebrew.sh`.** Fixture `SHA256SUMS` with four distinct 64-hex hashes (`aaaa…`, `bbbb…`, `cccc…`, `dddd…`) for the linux x86_64 tarball, both darwin tarballs and the dmg of version `1.2.3`. Run `render.sh 1.2.3 sums $tmp/tap`; assert both files exist, `ruby -c` accepts both, no `@…@` placeholder remains, the formula contains each of the three tarball URLs and hashes, the cask contains the dmg hash. Second case: a sums file lacking the dmg line makes `render.sh` fail and write no cask.
- [ ] **Step 2: Run `bash packaging/tests/homebrew.sh`.** Expected: FAIL.
- [ ] **Step 3: Write the templates and `render.sh`.** Formula: `class Cecilia < Formula`, `desc`, `homepage`, `version`, `license any_of: ["MIT", "Apache-2.0"]`; `on_macos` with `on_arm`/`on_intel` blocks (URL `https://github.com/kjmcnamara1/cecilia/releases/download/v@VERSION@/cecilia-v@VERSION@-{aarch64,x86_64}-apple-darwin.tar.gz`); `on_linux` with `depends_on arch: :x86_64`, `depends_on "alsa-lib"` and the linux tarball; `install` does `bin.install "cecilia"` and `doc.install "README.md", "CHANGELOG.md"`; `test` asserts `cecilia --version` output contains the version. Cask: `cask "cecilia"`, `version`, `sha256`, dmg url, `name`, `desc`, `homepage`, `depends_on macos: ">= :big_sur"`, `app "Cecilia.app"`, `binary "#{appdir}/Cecilia.app/Contents/MacOS/cecilia"`, a `postflight` that runs `/usr/bin/xattr -dr com.apple.quarantine` on the app (unsigned build; comment why) and a `zap trash` for the config directory (check which directory the code uses for macOS config with `grep -rn "ProjectDirs\|config_dir" crates/cecilia-app/src`). `render.sh`: POSIX sh; look up each hash with `grep " $asset\$" SHA256SUMS | cut -d' ' -f1`, fail with the asset name if empty, render both files with `sed` only after all hashes are found.
- [ ] **Step 4: Run `bash packaging/tests/homebrew.sh` and `shellcheck packaging/homebrew/render.sh`.** Expected: PASS, clean.
- [ ] **Step 5: Release job `homebrew`.** `needs: [meta, release]`, `if: needs.meta.outputs.prerelease == 'false'`: checkout this repo; checkout `kjmcnamara1/homebrew-cecilia` into `tap` with `token: ${{ secrets.HOMEBREW_TAP_TOKEN }}`; `gh release download "$GITHUB_REF_NAME" -p SHA256SUMS` (with `GH_TOKEN`); `packaging/homebrew/render.sh <version> SHA256SUMS tap`; commit `cecilia <version>` as `github-actions[bot]` and push (skip the commit when nothing changed).
- [ ] **Step 6: Commit** `feat(packaging): Homebrew formula and cask published to a tap`.

### Task 4: AppImage

**Files:**
- Create: `packaging/appimage/build.sh`, `packaging/tests/appimage.sh`
- Modify: `.github/workflows/release.yml` (job `appimage`)

**Interfaces:**
- Consumes: `packaging/linux/stage.sh DESTDIR PREFIX BINARY`.
- Produces: `packaging/appimage/build.sh VERSION BINARY OUT_DIR` assembles `OUT_DIR/AppDir` via `stage.sh "$OUT_DIR/AppDir" /usr BINARY`, then runs `${LINUXDEPLOY:-linuxdeploy-x86_64.AppImage} --appdir OUT_DIR/AppDir --desktop-file <AppDir>/usr/share/applications/cecilia.desktop --icon-file <AppDir>/usr/share/icons/hicolor/scalable/apps/cecilia.svg --output appimage` from `OUT_DIR` with `VERSION` and `LDAI_OUTPUT=cecilia-v$VERSION-x86_64.AppImage` exported.

- [ ] **Step 1: Write `packaging/tests/appimage.sh`.** A stub `LINUXDEPLOY` script records its arguments and environment to a file and creates the file named by `LDAI_OUTPUT`. Run `build.sh 0.0.1 stub-binary $tmp/out`; assert `AppDir/usr/bin/cecilia` is executable, the recorded arguments contain `--output appimage` and both `--desktop-file` and `--icon-file` paths that exist, `VERSION=0.0.1`, and `$tmp/out/cecilia-v0.0.1-x86_64.AppImage` exists.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Implement `build.sh`** as specified above (POSIX sh, `set -eu`).
- [ ] **Step 4: Run `bash packaging/tests/appimage.sh` and shellcheck.** Expected: PASS, clean.
- [ ] **Step 5: Release job `appimage`.** `continue-on-error: true`, `needs: [meta, build]`, ubuntu-22.04: download artifact `x86_64-unknown-linux-gnu`, untar, download `linuxdeploy-x86_64.AppImage` from `https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/` and `chmod +x`, `APPIMAGE_EXTRACT_AND_RUN=1 LINUXDEPLOY=$PWD/linuxdeploy-x86_64.AppImage packaging/appimage/build.sh <version> <binary> out`, upload `out/*.AppImage` as artifact `appimage`; add to the `release` job's `needs`.
- [ ] **Step 6: Commit** `feat(packaging): AppImage built by the release workflow`.

### Task 5: Flatpak

**Files:**
- Create: `packaging/flatpak/io.github.kjmcnamara1.cecilia.yml`, `packaging/flatpak/io.github.kjmcnamara1.cecilia.metainfo.xml`, `packaging/tests/flatpak.sh`
- Modify: `justfile` (`bump` maintains the metainfo), `.github/workflows/release.yml` (job `flatpak`), `.gitignore` (`packaging/flatpak/cargo-sources.json`)

**Interfaces:**
- Produces: the manifest builds the checkout (`type: dir`, `path: ../..`, `skip: [target, .git, test-corpus]`) offline from `cargo-sources.json` (generated by `flatpak-cargo-generator.py Cargo.lock`, never committed). For a Flathub submission the `dir` source is replaced by `type: git` with the release tag and commit (documented in Task 8).

- [ ] **Step 1: Write `packaging/tests/flatpak.sh`.** Asserts: the manifest parses (`ruby -ryaml`), its `app-id`, `command: cecilia`, `finish-args` contain `--socket=wayland`, `--socket=fallback-x11`, `--device=dri`, `--socket=pulseaudio`, `--filesystem=home`; `xmllint --noout` and `appstreamcli validate --no-net --pedantic` accept the metainfo (screenshots are warnings, not errors: use `appstreamcli validate --no-net` without `--pedantic` if pedantic fails only on them); the workspace version from `Cargo.toml` has a `<release version="X">` in the metainfo.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Write manifest and metainfo.** Manifest: runtime `org.freedesktop.Platform` `24.08`, sdk `org.freedesktop.Sdk`, `sdk-extensions: [org.freedesktop.Sdk.Extension.rust-stable]`, `command: cecilia`, the `finish-args` above plus `--share=ipc`; module `cecilia` (`buildsystem: simple`, `build-options` appending `/usr/lib/sdk/rust-stable/bin` to the path and `CARGO_HOME=/run/build/cecilia/cargo`, `build-commands`: `cargo --offline fetch --manifest-path Cargo.toml --verbose`, `cargo --offline build --release --locked -p cecilia`, `packaging/linux/stage.sh "" /app target/release/cecilia`, then rename the installed desktop entry, icon and (separately) install the metainfo to `/app/share/metainfo/` under the app ID and `sed` the desktop file's `Icon=` to the app ID); sources `dir` + `cargo-sources.json`. Metainfo: id, `metadata_license` `CC0-1.0`, `project_license` `MIT OR Apache-2.0`, name, summary, description, `launchable` desktop-id, `url` homepage/bugtracker (GitHub issues), `provides` binary `cecilia`, `categories`, `content_rating` `oars-1.1` empty, `<releases>` with the current version (`<release version="0.46.0" date="2026-11-12"/>`).
- [ ] **Step 4: `just bump`.** After the existing edits, insert `    <release version="$new" date="$(date +%F)"/>` as the first child of `<releases>` in the metainfo, and add the metainfo to the drift-check file list. Run a scratch copy (`cp -r` of the repo without `target/` into the scratchpad, add a line under `[Unreleased]`, `just bump patch`) and confirm the metainfo gains the release and `bash packaging/tests/flatpak.sh` still passes there.
- [ ] **Step 5: Run `bash packaging/tests/flatpak.sh`.** Expected: PASS.
- [ ] **Step 6: Release job `flatpak`.** `continue-on-error: true`, `needs: meta`, ubuntu-latest in container `ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-24.08` with `options: --privileged`: checkout; `pip install aiohttp toml tomlkit`, clone `flatpak/flatpak-builder-tools`, run `flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json`; `flatpak/flatpak-github-actions/flatpak-builder@v6` with `bundle: cecilia-v<version>.flatpak`, `manifest-path`, `cache-key: flatpak-${{ github.sha }}`; upload the bundle as artifact `flatpak`; add to `release` `needs`.
- [ ] **Step 7: Commit** `feat(packaging): Flatpak manifest, AppStream metainfo and a bundle in the release`.

### Task 6: Snap

**Files:**
- Create: `snap/snapcraft.yaml`, `snap/gui/cecilia.desktop`, `snap/gui/cecilia.svg` (byte copy of `packaging/linux/cecilia.svg`), `packaging/tests/snap.sh`
- Modify: `.github/workflows/release.yml` (jobs `snap`, `snap-publish`)

**Interfaces:**
- Produces: snap `cecilia` with apps `cecilia` (terminal UI, `bin/cecilia`) and `gui` (`bin/cecilia --gui`, installed as command `cecilia.gui`, carries the desktop entry).

- [ ] **Step 1: Write `packaging/tests/snap.sh`.** Asserts: `snap/snapcraft.yaml` parses (`ruby -ryaml`), `name: cecilia`, `base: core24`, `confinement: strict`, `grade: stable`, `adopt-info: cecilia`; `apps.gui.command` is `bin/cecilia --gui`; `apps.gui.plugs` include `desktop`, `wayland`, `x11`, `opengl`, `audio-playback`, `audio-record`, `home`; `cmp snap/gui/cecilia.svg packaging/linux/cecilia.svg`; `desktop-file-validate snap/gui/cecilia.desktop` passes and its `Exec=` starts with `cecilia.gui`.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Write the files.** `snapcraft.yaml`: `extensions: [gnome]` on both apps (supplies Wayland, X11 and Mesa), `summary`, `description`, `license: MIT OR Apache-2.0`, `title`; part `cecilia` with `plugin: rust`, `source: .`, `build-packages: [libasound2-dev, pkg-config]`, `stage-packages: [libasound2t64]`, `rust-path: [crates/cecilia]`, and `override-pull` that runs `craftctl default` then `craftctl set version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)`. `snap/gui/cecilia.desktop`: copy of `packaging/linux/cecilia.desktop` with `Exec=cecilia.gui %F` and `Icon=${SNAP}/meta/gui/cecilia.svg`.
- [ ] **Step 4: Run `bash packaging/tests/snap.sh`.** Expected: PASS.
- [ ] **Step 5: Release jobs.** `snap` (`continue-on-error: true`, `needs: meta`, ubuntu-latest): checkout, `snapcore/action-build@v1` (id `build`), upload `${{ steps.build.outputs.snap }}` as artifact `snap`; add to `release` `needs`. `snap-publish` (`needs: [meta, release, snap]`, `if: needs.meta.outputs.prerelease == 'false'`): download artifact `snap`, `snapcore/action-publish@v1` with `env: SNAPCRAFT_STORE_CREDENTIALS: ${{ secrets.SNAPCRAFT_STORE_CREDENTIALS }}`, `snap: <the file>`, `release: stable`.
- [ ] **Step 6: Commit** `feat(packaging): Snap package built and published by the release workflow`.

### Task 7: cargo-binstall

**Files:**
- Modify: `crates/cecilia/Cargo.toml`
- Create: `packaging/tests/binstall.sh`

- [ ] **Step 1: Write `packaging/tests/binstall.sh`.** `cargo metadata --no-deps --format-version 1 | jq` on the `cecilia` package: `metadata.binstall["pkg-url"]` equals `{ repo }/releases/download/v{ version }/cecilia-v{ version }-{ target }.{ archive-format }`, `["bin-dir"]` equals `cecilia-v{ version }-{ target }/{ bin }{ binary-ext }`, `["pkg-fmt"]` is `tgz`, and `overrides["x86_64-pc-windows-msvc"]["pkg-fmt"]` is `zip`.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Add `[package.metadata.binstall]`** with those values (and the override table) to `crates/cecilia/Cargo.toml`.
- [ ] **Step 4: Run it, then `cargo metadata --no-deps >/dev/null`.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(packaging): cargo-binstall metadata`.

### Task 8: Documentation and housekeeping

**Files:**
- Create: `docs/INSTALL.md`
- Modify: `README.md` (Installation section, Platforms bullet, "Not there yet" bullet), `AGENTS.md` (Releases), `CHANGELOG.md` (`[Unreleased]`), `TODO.md` (Packaging/Publishing), `docs/SPEC.md` only if it states packaging status that is now wrong (grep first)

- [ ] **Step 1: `docs/INSTALL.md`.** Every channel with the exact command and its caveats: curl script and its environment variables; macOS (`.dmg` drag-and-drop, Gatekeeper: unsigned, right-click Open or `xattr -dr com.apple.quarantine /Applications/Cecilia.app`; `brew install --cask kjmcnamara1/cecilia/cecilia`; formula `brew install kjmcnamara1/cecilia/cecilia`; the app launches the GUI, `cecilia` on the command line is the TUI; files open from the terminal); Linux (AppImage: `chmod +x`, no desktop integration without a helper; Flatpak: `flatpak install` the release bundle, config in `~/.var/app/io.github.kjmcnamara1.cecilia/`, SoundFont must be under the home directory; Snap: `snap install cecilia`, `cecilia.gui`, config under `~/snap/cecilia/current`, `home` plug sees only non-hidden files; deb, rpm, AUR as before); `cargo binstall cecilia`; `cargo install`; Windows (Scoop, Winget, Chocolatey as already written, still untested). A table at the top compares channels.
- [ ] **Step 2: README.** Replace "cecilia is not packaged yet" with a short install table linking `docs/INSTALL.md` and keep the source build; fix the Platforms bullet (binary releases now exist) and drop "binary releases, distribution packages" from "Not there yet"; replace the macOS/Windows "Packaged builds do not exist yet" paragraph accordingly (macOS packaged, Windows manifests untested). Keep the untested-on-real-hardware caveats.
- [ ] **Step 3: AGENTS.md Releases.** List the new release artifacts and jobs, the secrets (`CARGO_REGISTRY_TOKEN`, `AUR_SSH_PRIVATE_KEY`, `HOMEBREW_TAP_TOKEN`, `SNAPCRAFT_STORE_CREDENTIALS`), the one-time manual steps (create `kjmcnamara1/homebrew-cecilia`, `snapcraft register cecilia`, submit the Flatpak to Flathub with a git-source manifest, which replaces the `dir` source), that Flatpak/Snap/AppImage jobs are first exercised by a `vX.Y.Z-rc.N` tag, and that `just bump` also maintains the metainfo release list. Mention `packaging/check.sh`.
- [ ] **Step 4: CHANGELOG `[Unreleased]` Added.** One bullet per channel (macOS `.app`/`.dmg`, install script, Homebrew tap, AppImage, Flatpak, Snap, cargo-binstall). TODO.md: tick what is done, add open items: first `-rc` run of the Flatpak/Snap/AppImage jobs; Flathub submission and screenshots; macOS notarization (needs Apple Developer account); Finder open-file events; Nix flake; run the `.dmg`/installer on a real Mac.
- [ ] **Step 5: Verify.** `just ci` (fmt, clippy, tests, packaging checks) passes; `rg -n "not packaged yet|Packaged builds do not exist"` finds nothing; internal links resolve.
- [ ] **Step 6: Commit** `docs: installation channels, release secrets and changelog`.

## Status

- [x] Task 1: macOS `.app` and `.dmg`
- [x] Task 2: curl install script
- [x] Task 3: Homebrew tap
- [x] Task 4: AppImage
- [x] Task 5: Flatpak
- [x] Task 6: Snap
- [x] Task 7: cargo-binstall
- [x] Task 8: Documentation
- [x] Final review done (one fix pass); [ ] push and open the PR (do not merge; the user reviews)

Notes: branch renamed from `docs/macos-install-guide` to `feat/easy-install` (it held no commits of its own).

Final review fixes (one pass): the AppImage opens the window on a double-click via a custom
`AppRun` (test: `packaging/tests/appimage.sh`), the Flatpak targets runtime/container 25.08,
`snap-publish` is `continue-on-error`. Deferred minors: see the PR description.
