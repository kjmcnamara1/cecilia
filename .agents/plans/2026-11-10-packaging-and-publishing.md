# Packaging & Publishing Plan

> **Status: PLANNED (2026-11-10), nothing started.** Source: the "Packaging/Publishing" list in `TODO.md`.
> **For agentic workers:** work phase by phase; each phase is independently mergeable. Use a feature branch per phase and ask the user before merging to `main` (see AGENTS.md).

## Goal

Ship cecilia (`target/release/cecilia`, TUI by default, `--gui` window) to users without a Rust toolchain:
CI that builds/tests on every push, tagged releases that publish binaries and distro packages, and the
package-manager manifests from `TODO.md` (crates.io, AUR, .deb, .rpm, Scoop, Winget, Chocolatey, macOS).

Out of scope: auto-update, code signing/notarization (macOS and Windows ship unsigned at first; note the
SmartScreen/Gatekeeper warning in the README), Flatpak/Snap/AppImage (not in TODO), bundling a SoundFont.

## Context

- Workspace version is in the root `Cargo.toml` (`[workspace.package] version`, currently 0.44.0). Releases are tagged `vX.Y.Z` (`chore(release): vX.Y.Z` commits; tags exist up to the latest release). `CHANGELOG.md` is Keep-a-Changelog.
- The shipped binary is the `cecilia` crate (`crates/cecilia`), which links the TUI **and** GUI (vello/wgpu/winit/resvg, `crates/cecilia-gui`). Audio is `cpal` 0.15 (ALSA on Linux, needs `libasound2-dev` / `alsa-lib` to build); GUI needs a Vulkan/GL driver at runtime. `libc` is used in `cecilia-app`.
- Assets are compiled in (`include_bytes!` fonts from `assets/fonts`, `include_str!` themes), so the binary is self-contained. The SoundFont is user-supplied (`audio.soundfont`); a built-in synth is the fallback.
- Existing Linux packaging files: `packaging/linux/{cecilia.desktop,cecilia.svg,cecilia-mime.xml,install.sh,uninstall.sh}`. `install.sh` already shows the install layout (bin, desktop entry, scalable icon, mime xml). Reuse these in every Linux package; do not duplicate them.
- No `.github/` exists yet. Remote is `git@github.com:kjmcnamara1/cecilia.git` (`gh` is authed as `kjmcnamara1`; the repo did not exist on GitHub as of 2026-11-10 and must be created). Publishing is automated through GitHub Actions. `cecilia` appeared free on crates.io (user check). The user has Windows and macOS VMs and will clone and build there. Phase 0 adds `repository`, `homepage`, `readme`, `keywords`, `categories` to the crate manifests.
- `test-corpus/` is git-ignored; 4+ tests read it and fail without it. CI must either skip them or the tests must tolerate a missing directory (decide in Phase 1; prefer making those tests skip when the directory is absent, since the corpus can't be committed).
- Licence: `MIT OR Apache-2.0`, both licence files at the root; include both in every package.

## Steps

### Phase 0 — Repo metadata (prerequisite)

1. Create the GitHub repo `kjmcnamara1/cecilia` (`gh repo create`) and push `main` and tags (remote `origin` is already added in this worktree).
2. Add `repository`, `homepage`, `readme`, `keywords`, `categories` to `[workspace.package]` and inherit in each crate (`x.workspace = true`). Verify: `cargo metadata --no-deps` shows them.
3. Add a `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip = true`, maybe `panic` left default) after measuring size/speed; verify `cargo build --release` still works and note the binary size delta.

### Phase 1 — GitHub Actions (CI)

1. `.github/workflows/ci.yml`: on push/PR, ubuntu-latest, install `libasound2-dev pkg-config`, `dtolnay/rust-toolchain` stable + `Swatinem/rust-cache`, then `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Add a `rust-version` (1.85) check job with `cargo check --workspace`.
2. Make `test-corpus`-dependent tests skip cleanly when the dir is missing (find with `grep -rn test-corpus crates`); verify with `cargo test --workspace` in a worktree lacking the directory.
3. Add a Windows and macOS `cargo check --workspace` matrix job (this is what proves the later release phases are feasible).
4. Verify: push a branch, all jobs green.

### Phase 2 — Release workflow (binaries)

1. `.github/workflows/release.yml`, triggered by tags `v*`. Matrix builds `cargo build --release --locked -p cecilia` for:
   - `x86_64-unknown-linux-gnu` (build on an older Ubuntu LTS for glibc compatibility), optionally `aarch64-unknown-linux-gnu`
   - `x86_64-pc-windows-msvc`
   - `aarch64-apple-darwin` and `x86_64-apple-darwin`
2. Archive per target: `cecilia-vX.Y.Z-<target>.tar.gz` (`.zip` on Windows) containing the binary, `LICENSE-*`, `README.md`, `CHANGELOG.md`, and on Linux `packaging/linux/*`. Generate `SHA256SUMS`.
3. Create the GitHub Release (`softprops/action-gh-release` or `gh release create`) with the notes taken from the matching `CHANGELOG.md` section.
4. A guard step: the tag must equal `v` + workspace version, else fail.
5. Verify: tag a pre-release `v0.44.1-rc.1` on a test branch/fork and download each artifact; run `cecilia --version` (Linux locally; others via CI smoke step `./cecilia --version`).

### Phase 3 — crates.io

1. The binary depends on 8 path crates; each must be published (in dependency order: core → formats/convert/engrave/audio → app → tui/gui → cecilia) with versioned path dependencies (`cecilia-core = { path = "...", version = "0.44.0" }` in `[workspace.dependencies]`).
2. Check the crate name `cecilia` is free on crates.io; if taken, publish as `cecilia-cli` with `[[bin]] name = "cecilia"`. Also decide whether library crates should be public API (they will be published; mark clearly as internal in their descriptions) .
3. Check package size limits (10 MB) given `assets/fonts` and fonts' licences (SIL OFL, include licence text); add `include`/`exclude` as needed. Verify with `cargo package --list -p <crate>` and `cargo publish --dry-run`.
4. Add a `publish` job to `release.yml` (after the binaries succeed) using a `CARGO_REGISTRY_TOKEN` secret, publishing in order with a wait between crates (`cargo publish` waits for index by default in recent cargo). The user must create the token.
5. Document `cargo install cecilia` in the README (note build deps: `alsa-lib`, `pkg-config`).

### Phase 4 — Linux packages

Shared: a script/Makefile target `packaging/linux/stage.sh DESTDIR PREFIX` that installs binary + desktop entry + icon + mime xml + licences into a staging tree, so every package format uses one layout (refactor `install.sh` to call it rather than copying its install lines).

1. **PKGBUILD (AUR, source)** `packaging/aur/cecilia/PKGBUILD`: `makedepends=(cargo)`, `depends=(alsa-lib vulkan-icd-loader)` (verify with `namcap`), build with `cargo build --release --frozen`, install via the stage script. Verify: `makepkg -f` in a clean chroot/container (`extra-x86_64-build`) and `namcap`.
2. **cecilia-bin (AUR)** `packaging/aur/cecilia-bin/PKGBUILD`: downloads the Phase 2 Linux tarball and checks its sha256 (`provides=(cecilia)`, `conflicts=(cecilia)`). A release step bumps `pkgver` and checksums (script `packaging/aur/bump.sh`); pushing to the AUR is manual or via an SSH-key secret (user decision).
3. **.deb**: `cargo-deb` metadata in `crates/cecilia/Cargo.toml` (`[package.metadata.deb]`, assets from `packaging/linux`, depends `$auto, libasound2`). Verify: `cargo deb`, then `dpkg -c` and install in a Debian container.
4. **.rpm**: `cargo-generate-rpm` metadata (`[package.metadata.generate-rpm]`, same assets, requires `alsa-lib`). Verify: build, `rpm -qlp`, install in a Fedora container.
5. Phase 2 workflow builds `.deb`/`.rpm` on the Linux job and attaches them to the release.

### Phase 5 — Windows

1. Confirm the Windows build runs: GUI window works with `--gui`, TUI works in Windows Terminal; fix any Unix-only code (`libc` use in `cecilia-app`, config/paths via `directories`). This is likely the biggest unknown; do it first and record findings here. Decide whether the release binary uses `windows_subsystem = "windows"` (it can't: TUI needs a console) — keep the console subsystem.
2. **Scoop**: a manifest `packaging/scoop/cecilia.json` (url to the release zip, hash, `bin`, `checkver` + `autoupdate` from GitHub) published in a user bucket repo (`scoop-bucket`) or the Extras bucket by PR.
3. **Winget**: manifests under `packaging/winget/` (portable zip installer type, `wingetcreate` can generate them; submit PR to `microsoft/winget-pkgs` per release, can be automated with `vedantmgoyal9/winget-releaser`).
4. **Chocolatey**: `packaging/choco/cecilia.nuspec` + `tools/chocolateyinstall.ps1` (download the zip, checksum); push with an API key secret; note the moderation queue.
5. Verify each in a Windows VM/CI job: install, run `cecilia --version`, uninstall.

### Phase 6 — macOS

1. Check the build on both architectures (CI from Phase 1); fix platform-specific issues (audio via CoreAudio comes with cpal, GUI via Metal through wgpu).
2. Ship the tarball from Phase 2; optionally a `.app` bundle with `Info.plist` and the file-type associations (`.gp*`, `.ccc`, `.gabc`) plus a `.dmg`. Unsigned: document `xattr -d com.apple.quarantine`.
3. Optional follow-up: a Homebrew tap formula (`brew install owner/tap/cecilia`) — cheap once the tarballs and sha256 exist; add it to `TODO.md` rather than building it now.

## Code-review findings to consider before release (2026-11-10, read from code, not all reproduced)

- Playback caches (`playback.rs`) are keyed by buffer index and not invalidated by `restore()` in `workspace.rs`, `close_last_view`, or `open()` reusing slot 0: a workspace switch can play the old file.
- `cecilia-audio/src/player.rs`: `audio_pos` is not resynced when speed/transpose change mid-play.
- Unbounded reads: GPX `read_filesystem` (`gpx.rs:89,141`) and zip entries (`native.rs`, `gpif.rs`, `musicxml.rs`); GABC `nabc-lines` overflow (`gabc.rs:333`).
- `:w FILE` overwrites existing files without `!`; file writes are not atomic (`cecilia-formats/src/lib.rs:197`, `workspace.rs`).
- Windows: AltGr arrives as Ctrl+Alt (`cecilia-gui/src/keys.rs`); `pid_alive` is Linux-only (`swap.rs`); SoundFont discovery is Linux paths only (`config.rs`); macOS bundle needs `NSMicrophoneUsageDescription` for the tuner.
- Unverified: GP3/4 grace-note field order (`gp.rs:926`), checked only from memory of other implementations.

## Decisions to settle with the user

- Which of the code-review findings above to fix before the first release.
- Whether to publish the library crates (they must be, for `cargo install cecilia`) and confirm the `cecilia` name at publish time.
- Which targets are in the first release (suggest: Linux x86_64 + Windows x86_64 + macOS aarch64, add the rest later).
- Whether to sign/notarize later (needs paid accounts).

## Status

- [x] Phase 0 — repo metadata, release profile (done 2026-11-11 on `chore/packaging-phase-0`: `[workspace.package]` has repository/homepage/readme/keywords/categories, inherited by all 9 crates; `[profile.release]` is `lto = "thin"`, `codegen-units = 1`, `strip = true`: binary 32.5 MB -> 23.4 MB, clean release build ~2 min on this machine; runtime speed not measured. Still open: create the GitHub repo and push (step 1), waiting on the user's visibility choice)
- [ ] Phase 1 — CI workflow
- [ ] Phase 2 — release workflow (binaries, checksums, release notes)
- [ ] Phase 3 — crates.io
- [ ] Phase 4 — Linux: stage script, PKGBUILD, cecilia-bin, .deb, .rpm
- [ ] Phase 5 — Windows: build fixes, Scoop, Winget, Chocolatey
- [ ] Phase 6 — macOS: build, tarball/.app/.dmg

Notes / blockers: none yet. Open questions are under "Decisions to settle with the user".
