# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Docs

- [ ] Static site for documentation (Zensical)
  - [ ] Github pages
- [x] Document the specification for the native file (.ccc / .ccm) in a section in docs/SPEC.md
- [x] Rewrite README.md for audience=user

### Packaging/Publishing

- [x] Github Actions
- [x] crates.io
- [ ] Linux Releases
  - [x] PKGBUILD for AUR: written and built with `makepkg`; the release workflow now pushes it (needs the `AUR_SSH_PRIVATE_KEY` secret and the AUR packages registered); verify after the next release
  - [x] cecilia-bin for AUR: same
  - [x] PKGBUILD for cecilia-git: written (version from `git describe`, untested with `makepkg`); the release workflow pushes it too
  - [ ] Wait on AUR adoption requests for cecilia and cecilia-git
  - [ ] .deb and .rpm: built by the release workflow; install-test the CI-built files
- [ ] More Linux packages
  - [ ] Push a `vX.Y.Z-rc.N` tag to exercise the AppImage, Flatpak and Snap jobs (written without being run), fix what breaks, install-test each
  - [ ] `snapcraft register cecilia` and the `SNAPCRAFT_STORE_CREDENTIALS` secret; check the snap's file access and audio
  - [ ] Flathub: submit to `flathub/flathub` (git-source manifest, see `docs/INSTALL.md`), add screenshots to the metainfo
  - [ ] Nix flake (`rustPlatform.buildRustPackage`; needs wayland, vulkan-loader, libxkbcommon, alsa-lib at run time)
  - [ ] Run the install script on a clean Linux and macOS machine after the first release
  - [x] `cargo binstall` metadata (verify with `cargo binstall cecilia` after the first release)
- [ ] Windows Release
  - [ ] Run the release zip on a Windows machine: `cecilia --version`, the TUI in Windows Terminal, `cecilia --gui`, audio playback, the file picker, the clipboard (`clip.exe` / PowerShell)
  - [ ] `cecilia --gui` crashes on a Windows 11 QEMU/KVM VM (VirtIO GPU, display-only driver, no 3D): `STATUS_ACCESS_VIOLATION` inside `d3d10warp.dll` (Windows' software renderer) with `WGPU_BACKEND=dx12` and the WARP adapter forced (`WGPU_ADAPTER_NAME`); Vulkan has no driver there ("Couldn't create wgpu surface"). Native crash, so not a cecilia bug as far as known. The hosted `windows-latest` runner (no GPU) segfaults the same way in the headless `--screenshot` render (CI job `render-smoke`, 2026-11-11), so it is not window-specific. `ui.renderer` / `CECILIA_RENDERER`, the `auto` fallback (WARP is skipped) and the GPU-free `cpu` renderer now exist; remaining: retest on the real Windows VM with `ui.renderer cpu` (and `auto`). The TUI works on the same VM.
  - [ ] Scoop: `packaging/scoop/cecilia.json` is written but untested; run `packaging/bump-windows.sh` after the first release, test with `scoop install`, then publish to a bucket
  - [ ] Winget: `packaging/winget/` is written but untested; validate with `winget validate` and `wingetcreate`, then PR to `microsoft/winget-pkgs`
  - [ ] Chocolatey: `packaging/choco/` is written but untested; `choco pack`, test install, then push (needs an API key)
  - [ ] AltGr typing in `--gui` (`cecilia-gui/src/keys.rs`, logic unit-tested): on a German layout AltGr+Q must type `@`, not `<C-A-q>`; also `%WINDIR%\Fonts` fonts load
  - [ ] Notification history times show UTC on Windows (`notify.rs` `local_time` only uses the local zone on Unix)
  - [ ] Typed paths: `~\` and `\` separators in the path completion and workspace names (`complete.rs`, `workspace.rs`) only understand `/`
- [ ] MacOS Release
  - [x] `.app` and universal `.dmg` in the release workflow, `install.sh`, Homebrew tap (formula and cask): written; CI tests the bundle on macOS but nothing has run on a real Mac
  - [ ] Create `kjmcnamara1/homebrew-cecilia` and the `HOMEBREW_TAP_TOKEN` secret; test `brew install --cask` and `brew install` after the first release
  - [ ] Notarize the app (needs an Apple Developer account) so Gatekeeper and the official `homebrew-cask` accept it
  - [ ] Open files from Finder (double-click, drag onto the Dock icon): winit does not deliver the open-document Apple event, so `Info.plist` lists the file types but the app ignores the request
  - [ ] macOS SoundFont default paths, and the bundle's microphone prompt (`NSMicrophoneUsageDescription`) when opening the tuner
  - [ ] Never run on real hardware: the author's macOS VM (Intel, x86_64) has no Metal (Ghostty cannot start there) and no working audio output (CoreAudio: "unknown error"). Unverified: `--gui`, `--screenshot`, playback, the tuner and the microphone prompt, the Apple Silicon build. The TUI runs (Terminal.app and iTerm2) and renders; Homebrew does not support Intel Macs

### Refactor

#### Commands

- Always in lower-kebab-case
- Command can have a set of aliases
- Commands can have arguments

#### Pickers

- [ ] remove pick-* commands. just have :pick PICKER
- [ ] Need to be able to paste into pickers and commands (and dictate w/
      superwhisper)

##### Marks

- [x] Saved marks should display somewhere on the score
- [ ] Mark picker unecessary with which-key popup?

##### Tuning

This is how the # of stafflines is decided for tab notation

- [ ] Better interface for selecting/customizing tuning

### Tool Panes

- [ ] Add ability to create multitracks that are a combination of other tracks
- [ ] Toolbar
  - hamburger icon menu
  - Reorder/regroup icons
  - Change icons

#### Tracks

- [ ] "d" should delete the track
- [ ] "o" should add a new track
- [ ] "r" should rename the track (today `t`; `c` is the track colour, `i` the instrument)
- [ ] instrument should be preceeded by its icon

### Performance

- [ ] Compare CPU & mem usage to GP8 and MuseScoreStudio and Go Playalong

1. Redraw only the playhead and cursor area, or cache the engraved scene and just move the playhead overlay each frame. This is the main CPU fix.
2. Redraw only when the playhead has moved by at least a pixel or so, and lower the rate to 30 fps.
3. Measure memory first (e.g. with heaptrack, or by running with and without soundfont set) before changing anything.

### Chant

- [ ] Need to review the process of converting <-> between chant and modern and
      the rules used
- [ ] Automatic audio sync for chant (`:sync-detect` in a chant score): track the recording's
      pitch (YIN, `cecilia-audio/src/tuner.rs`, ~10 ms hops), align the chant's note pitches and
      lengths by DTW allowing a constant transposition, set a point at each syllable start snapped
      to onsets, marked as detected (phase 3c step 8).
- [ ] Note ids through GABC text editing, properly (phase 4 / plan step 5): the properties pane's
      text edit and `:gabc` already match reparsed notes to the old ones by longest common
      subsequence on pitch and signs (`Chant::match_ids`); phase 4's splice edits should keep
      ids exactly, and the match could use the syllable text and shape too.

### File Save/Export

- [x] Converting/exporting to chant (.gabc) files (`:modern-to-chant`, `cecilia convert x.gp5 x.gabc`)
- [ ] Write ASCII tab (`.txt`); `:w x.pdf` / `.wav` export from inside the editor
- [ ] Nothing should be lost when writing to .ccc / .ccm files
- [ ] Some data may be lost when writing to .gp, .musicxml, .gabc (there is no .txt writer yet)
- [ ] Model wide vibrato, the feedback harmonic and golpe (`docs/SPEC.md` §4.1): the GPIF reader drops them today

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Audio

- [ ] Add AI singing voice that can sing lyrics?

### Keymaps

- [ ] TODO: Deep dive on keymap architecture and how to define (config.toml seems lacking)
  - Should be dependent on mode and currently focused window/pane/staff

### UI

- [ ] Allow grouping scores in tabline into groups and ability to (auto)
      collapse/expand them
- [ ] Add ability to have multiple windows in the same workspace (not just views
      and panes)
  - :workspace-close will close all windows attached to the workspace
  - :quit will close the app and all workspaces/windows
  - starting a new instance of cecilia will attach to the currently running
    workspace by default
  - can still start "cecilia --workspace NAME" to open a window in
    a new/different workspace

### Voices

- [ ] Shortening or lengthening a beat in a voice should refill the rest of the bar with rests
      (or eat following rests), as Guitar Pro does; today the bar goes incomplete
- [ ] Voice-specific copy/paste of all voices of a selection at once (copy and paste work on the
      active voice), and voice stem-direction overrides per beat

### TUI

### GUI

- [ ] UI designs i like:
  - T3 Code (glassy popup)
  - Zed

### Scripting

- [ ] Add mlua for ability to script application and create plugins
- [ ] Add MCP server integration
