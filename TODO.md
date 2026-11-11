# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Docs

- [ ] Static site for documentation (Zensical)
  - [ ] Github pages
- [x] Document the specification for the native file (.ccc / .ccm) in a section in docs/SPEC.md
- [x] Rewrite README.md for audience=user

### Packaging/Publishing

- [ ] Github Actions
- [ ] crates.io
- [ ] Linux Releases
  - [ ] PKGBUILD for AUR
  - [ ] cecilia-bin for AUR
  - [ ] .deb
  - [ ] .rpm
- [ ] Windows Release
  - [ ] Scoop
  - [ ] Winget
  - [ ] Chocolatey
- [ ] MacOS Release

### Code review follow-ups (2026-11-10)

Left open by the pre-release code review; the rest was fixed (see `CHANGELOG.md`).

- [ ] Swap files: offer to recover or clean up `unnamed-<pid>-<idx>.swp` files from crashed
      sessions (`swap.rs`); two instances editing the same path share one swap file name.
- [ ] Audio callback (`cecilia-audio/src/player.rs`): dropped clips and sequences go to a reaper
      thread over `std::sync::mpsc`, which can still allocate; use a lock-free queue.
- [ ] `write_bytes` (`cecilia-formats/src/lib.rs`) still writes the legacy `.cc` format; make it
      read-only by default, as AGENTS.md says, and fix what depends on it in `cecilia-app`.
- [ ] MIDI writer: a time signature whose denominator is not a power of two is floored to a power of two; MIDI cannot store it.
- [ ] Undoing back to the saved state should not clear "modified" on a buffer whose file was
      deleted on disk.
- [ ] Regression tests for Guitar Pro 3/4 grace notes (field order was fixed from corpus
      evidence, not a spec) and for the saturating lyric-start and channel arithmetic in `gp.rs`
      (needs a crafted GP binary).
- [ ] `chant-clef` accepts `cb`/`fb` on any line 1–4; the docs list only `cb3`.
- [ ] Not tested on real Windows or macOS: AltGr handling (`cecilia-gui/src/keys.rs`),
      `%WINDIR%` fonts, macOS SoundFont paths, the macOS bundle's microphone permission
      (`NSMicrophoneUsageDescription`, needed for the tuner).
- [ ] `docs/SPEC.md` §4.1/§4.2 still say "parity" and "As built" for features nobody re-checked;
      lines 306–345 (bufferline, notification history) were not traced through the GUI code.
- [ ] `docs/config.example.toml` has no `[keymap.visual]` or `insert-tab`/`insert-slash`/
      `insert-chant` sections; the TODO "Tracks" list below calls `c` "change instrument" but `c`
      is track colour and `i` is instrument.
- [ ] `write` completion offers `pdf`, `svg`, `png`, `wav`, `flac` (`complete.rs` `WRITE_EXTENSIONS`)
      though `:w` cannot write them.

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
- [ ] "r" should rename the track
- [ ] "c" should change the instrument
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
