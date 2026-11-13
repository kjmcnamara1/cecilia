# Windows installer, icon and auto-updates

**Status: implemented on the branch (steps 1–8 done); nothing of it has run on Windows yet (see
the notes at the end). Not merged.**

Branch: `feat/windows-installer-updates`.

## Goal

1. The Windows `cecilia.exe` has an embedded icon (Explorer, taskbar, shortcuts) and the GUI window
   shows it.
2. A GUI launcher exe without a console window, so Start Menu shortcuts don't open an empty
   console next to the window.
3. A Windows installer built with **Velopack** (`cecilia-win-Setup.exe`) that installs per user,
   creates a Start Menu shortcut with the icon and an uninstall entry, and can update itself.
4. A new option `ui.auto_update` = `on` | `ask` | `notify` | `off` (default `notify`):
   - `on`: download a newer release in the background and apply it when cecilia exits (tell the
     user "vX ready, applies on restart").
   - `ask`: show a dialog "vX is available, install?"; yes = as `on`.
   - `notify`: only say a newer version exists, and how to update for this install.
   - `off`: no network access at all.
   - A `:update` command checks now (and installs, where self-update is possible) regardless of
     the option, except it still never self-updates a package-manager install.

**Rule:** self-update (`on`/`ask` installing) only happens for Velopack installs. Every other
install (AUR, .deb/.rpm, Scoop, winget, Chocolatey, `cargo install`, a hand-unpacked zip/tarball)
degrades `on`/`ask` to `notify`, with the right command: `scoop update cecilia`,
`winget upgrade kjmcnamara1.cecilia`, `choco upgrade cecilia`, `cargo install cecilia`,
"your package manager" for `/usr/bin`, else a link to the releases page.

Out of scope: macOS/Linux Velopack packages, Windows file associations, code signing, switching
winget away from the zip, a PATH entry for the Velopack install.

## Context

- Binary crate: `crates/cecilia` (`src/main.rs`, ~630 lines, clap CLI; `--gui` opens the window,
  default is the TUI). It is published to crates.io (`cargo publish --workspace`), so **anything
  `build.rs` reads must live inside `crates/cecilia/`** (e.g. `crates/cecilia/assets/cecilia.ico`),
  not in `packaging/` or `assets/`.
- Icon sources: `assets/cecilia-transparent.svg` (use this; `assets/cecilia.png` is RGB with no
  alpha). `magick`/`rsvg-convert` exist locally to generate a multi-size `.ico` (16, 24, 32, 48,
  64, 128, 256) once; commit the `.ico`.
- Window creation: `crates/cecilia-gui/src/lib.rs:646` (`with_title`, Wayland/X11 `with_name`).
  `resvg` is already a dependency of cecilia-gui. On Windows, `winit::platform::windows::IconExtWindows::from_resource`
  can load the exe's icon resource; elsewhere rasterize the SVG (X11 uses it; Wayland ignores it
  and uses the `.desktop` file).
- GUI exe name: **`ceciliag.exe`** (g for GUI). Do not name a bin `cecilia-gui`: it collides
  with the `cecilia-gui` library's output/PDB names on Windows. Add it as a second `[[bin]]` in
  `crates/cecilia` with `#![cfg_attr(windows, windows_subsystem = "windows")]`, always launching
  the GUI. Move what both bins share out of `main.rs` (e.g. a `run(args)` in a `lib.rs`) rather
  than duplicating it.
- Options registry: `crates/cecilia-app/src/options.rs` (`Kind::Choice`); every option needs a
  row in `docs/OPTIONS.md` and an entry in `docs/config.example.toml` (tests fail on drift). New
  command → `docs/COMMANDS.md`.
- Notifications: `crates/cecilia-app/src/notify.rs`; dialogs: `crates/cecilia-app/src/dialog.rs`.
  Both frontends render them already.
- Release: `.github/workflows/release.yml` builds `x86_64-pc-windows-msvc` and zips `cecilia.exe`.
  `SHA256SUMS` covers `cecilia*`. Windows manifests: `packaging/scoop/cecilia.json` (already has a
  `shortcuts` entry pointing at `cecilia.exe --gui`), `packaging/winget/*` (zip, portable),
  `packaging/choco/tools/chocolateyinstall.ps1`. `just bump` rewrites versions in these; keep
  its patterns working (check the `justfile`).
- No HTTP client in the tree yet. Add `ureq` (rustls, json) for the GitHub release check. Use the
  existing `semver` (in Cargo.lock) for version comparison if convenient.

### Velopack (verify the details on docs.rs/velopack and docs.velopack.io)

- Rust crate `velopack`, as a **Windows-only** dependency (`[target.'cfg(windows)'.dependencies]`).
  Call `VelopackApp::build().run()` first thing in `main` of **both** bins (hooks run during
  install/update/uninstall and exit).
- `UpdateManager::new(source, None, None)` fails when not installed by Velopack: that is the
  "self-updatable?" detection. Source: `HttpSource` on
  `https://github.com/kjmcnamara1/cecilia/releases/latest/download` (latest skips pre-releases),
  or a GitHub source if the crate has one. `check_for_updates` → `download_updates` →
  `apply_updates_and_restart` / `wait_exit_then_apply_updates`.
- CI: `dotnet tool install -g vpk`; `vpk download github --repoUrl …` (previous release, for
  deltas; tolerate none), then `vpk pack --packId cecilia --packVersion <v> --packDir <dir with
  ceciliag.exe, cecilia.exe, licenses> --mainExe ceciliag.exe --icon crates/cecilia/assets/cecilia.ico
  --packTitle cecilia --shortcuts StartMenuRoot`; upload everything in `Releases/` to the GitHub
  release with the other assets (the feed `releases.win.json` and the `.nupkg` must be release
  assets). Velopack checks package hashes from the feed, fetched over HTTPS.

### Where code goes

- `crates/cecilia-app/src/update.rs`: the frontend-independent behaviour, with tests: the option,
  install-kind detection from the exe path (pure function over a path, testable), the decision
  table (setting × install kind × newer version → action), message texts, the background check
  thread delivering a result to `App` (follow how other background work reports into `App`), the
  `:update` command, the ask dialog. Velopack calls behind `#[cfg(windows)]`.
- Frontends only render existing notifications/dialogs; touch them only if something is missing.

## Steps

1. Icon: generate `crates/cecilia/assets/cecilia.ico`; `crates/cecilia/build.rs` embeds it with
   `winresource` only when `CARGO_CFG_TARGET_OS == "windows"` (build-dependency). Set the window
   icon in `cecilia-gui/src/lib.rs`. Verify: `cargo build`, `cargo package -p cecilia --list`
   includes the `.ico`.
2. `ceciliag` bin: shared code into `crates/cecilia/src/lib.rs`, second `[[bin]]`. Verify:
   `cargo build -p cecilia` builds both; `target/debug/ceciliag --help`-equivalent behaviour sane.
3. `ui.auto_update` option + docs rows. Verify: options/docs drift tests.
4. `update.rs`: detection, decision table, GitHub check via `ureq`, notify path, `:update` command
   (+ COMMANDS.md). Unit tests for detection and the decision table (no network in tests).
5. Velopack integration (`cfg(windows)`): `VelopackApp` in both mains, on/ask paths, apply on exit.
   Verify it compiles for Windows if a target is available (`cargo check --target
   x86_64-pc-windows-gnu` if installed; otherwise rely on CI and say so).
6. Release workflow: build both bins, add `ceciliag.exe` to the zip, Velopack pack + upload,
   include Setup.exe in `SHA256SUMS`. Scoop shortcut → `ceciliag.exe` (no `--gui`); Chocolatey
   `Install-ChocolateyShortcut` for `ceciliag.exe` (and remove it in an uninstall script, plus
   `.ignore` the `ceciliag.exe` shim / or `.gui` file so choco shims it as GUI).
7. Docs: README install section (Windows installer), CHANGELOG `[Unreleased]`.
8. `cargo fmt`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`, `just ci`.

## Status

- [x] 1 Icon (`cc2bbcb`)
- [x] 2 ceciliag bin (`45dd719`)
- [x] 3 option (in `5f3adc8`)
- [x] 4 update.rs + :update (in `5f3adc8`)
- [x] 5 Velopack (in `5f3adc8`)
- [x] 6 release workflow + manifests (`b935cd2`)
- [x] 7 docs (README, CHANGELOG, TODO)
- [x] 8 checks: `cargo fmt`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`,
  `just ci` all pass on Linux (msrv step skipped: no 1.89 toolchain installed)

Notes: default `notify` chosen by the main agent (the user specified the four values, not the
default); a check at startup is one HTTPS GET to GitHub, at most once per day (store the last
check time with other persisted state, if there is a place for it; otherwise every start).

### What was done, and changes from the plan

- Icon (user: the lily everywhere): `crates/cecilia/assets/cecilia.ico` (16–256 px, all entries
  PNG, 60 KB) rendered from `crates/cecilia-gui/assets/lily.svg` (`rsvg-convert` per size, then
  packed into an ICO with PNG entries). `build.rs` embeds it with
  `winresource` (`default-features = false`, icon ID 1) only for a Windows target; a resource
  compile failure is a build *warning*, not an error. The GUI loads it with
  `Icon::from_resource(1)` (window and taskbar icon) on Windows; on X11 it rasterizes
  `crates/cecilia-gui/assets/lily.svg` at 64 px. The Linux `.desktop` icon
  (`packaging/linux/cecilia.svg`) is the same lily.
- ceciliag (renamed from `ceciliaw` at the user's request; "g" for GUI): `crates/cecilia/src/bin/ceciliag.rs` (auto-discovered, no `[[bin]]` table needed);
  the CLI is `crates/cecilia/src/lib.rs` (`cecilia::main(force_gui)`, `cecilia::show_error`).
  ceciliag turns a clap usage error into an error (help/version still print and exit) and shows
  errors with `cecilia_app::dialog::error_box` (rfd message box; cecilia now enables
  cecilia-app's `dialog` feature itself). ceciliag is also built and installed on Linux/macOS
  (`cargo install`), where it is just `cecilia --gui`.
- Option: `ui.auto_update` is a typed enum (`config::AutoUpdate`, like `RendererMode`), so an
  invalid value in the file is a parse error (as for `ui.renderer`), not a warning.
- update.rs: `InstallKind::detect(exe, velopack)` (path rules: `scoop/apps|shims`,
  `winget/packages|links`, `chocolatey/lib|bin`, `.cargo/bin`, `/usr/bin` or `/bin` → System,
  else Manual; `/usr/local` is Manual), `action()`, `check_due()`, `is_newer()` (semver).
  The thread reports `Event`s; `App::poll_update` (from `poll_config`) waits while any prompt is
  open or the command line is in use. `App::question_open()` is shared with `external.rs`.
  Startup check: `App::check_for_updates_on_start()` from `crates/cecilia/src/lib.rs` (not in
  screenshot runs); the last *successful* startup check's Unix time is in the state dir file
  `update-check`. A failed startup check is silent; `:update` reports errors. `notify` on a
  Velopack install says ":update installs it". The cargo message is
  `cargo install --locked cecilia`.
- Velopack (`crates/cecilia-app/src/update/velopack.rs`, Windows only, `velopack = "1.2"` →
  1.2.161): `HttpSource` on `…/releases/latest/download` (not `GithubSource`: it is not clear how
  that handles the earlier releases without a feed; HttpSource is deterministic, and a missing
  delta falls back to the full package). The downloaded `UpdateInfo` waits in a static until
  `apply_on_exit()` (end of `cecilia::main`) calls `wait_exit_then_apply_updates(silent, no
  restart)` — calling it at download time would not work: the updater gives up after 60 s. If
  that fails, `VelopackApp`'s default auto-apply-on-startup installs it at the next start.
- Release workflow: `actions/setup-dotnet` (8.0.x), `dotnet tool install vpk --version <velopack
  version from Cargo.lock> --tool-path .vpk`, `vpk download github` (failure tolerated), `vpk
  pack … --mainExe ceciliag.exe --shortcuts StartMenuRoot --noPortable`; uploads
  `cecilia-win-Setup.exe`, `releases.win.json` and `cecilia-<v>-*.nupkg` (not `RELEASES` or
  `assets.win.json`). `SHA256SUMS` already covered `cecilia*`. Pre-releases get an installer too,
  but installed copies never see them (`latest` skips pre-releases). The Velopack package
  version is the tag (`0.47.0-rc.1`), not Cargo.toml's `0.47.0`, so an rc install upgrades to
  the final release.
- Pre-release flow (first: `v0.47.0-rc.1` from the feature branch): Cargo and packaging files
  at 0.47.0 via `just bump minor`, but the CHANGELOG entries stay under `[Unreleased]` (the
  release notes fall back to it); the `## [0.47.0]` heading is added by hand at the final
  release, since `just bump` will then say "already at 0.47.0".
- Chocolatey: a `ceciliag.exe.gui` file makes its shim a GUI one (instead of `.ignore`), a
  Start Menu shortcut in `CommonPrograms`, and `chocolateyuninstall.ps1` removes it. `just bump`
  checked in a scratch copy: still works.

### Verified / not verified

- Verified on Linux: build, all tests, clippy, `just ci`; `cargo package -p cecilia --list`
  includes `assets/cecilia.ico`; `ceciliag --version` and usage errors.
- Windows: the full workspace cannot be checked here (`signalsmith-stretch` needs MSVC's C++
  headers and `lib.exe`). The Windows-only Rust code was type-checked for
  `x86_64-pc-windows-msvc` in scratch crates (clang-cl, llvm-lib and stub CRT headers for ring):
  `update/velopack.rs` with velopack 1.2.161, the whole `update.rs` against stubbed `App`
  types (clippy clean), and the winit `from_resource` icon code.
- Never run: `build.rs` on Windows (winresource needs `rc.exe` from the Windows SDK, present on
  the hosted runners), the installer, the update flow, `vpk` in CI, the Chocolatey and Scoop
  shortcut changes. Added to TODO.md under "Windows Release".

### Open questions

- winget still installs the zip as a portable `cecilia` alias only (no `ceciliag` alias, no
  shortcut): out of scope per the plan.
- The Velopack `vpk pack` VelopackApp check (`--skipVeloAppCheck` not passed) is expected to find
  `VelopackApp::build().run()` in ceciliag.exe; if CI rejects it, check how vpk detects it in
  Rust binaries.
