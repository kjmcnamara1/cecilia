# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### AGENTS.md

- [x] Run /init to generate AGENTS.md
- [x] Use conventional commits
- [ ] Implement new features in feature branches before merging back into main.
      Ask me for when to do so.
- [ ] Use semantic versioning and tag commits for releases. I will say when to
      upgrade to first major version 1.0.0
- [ ] Keep CHANGELOG.md up to date

### Progress Bar

- [ ] FIX: Change style of progress bar to look like Go Playalong
      screenshot. Same background as the score (paper). Get rid of bluishness.
- [ ] FIX: Halve the width of the red indicator in the progress bar in the gui

### Engraving

- [ ] TODO: Clean up engraving bugs

### Tool Panes

- [ ] FIX: The audio tool should be referenced as the waveform tool.
- [ ] FIX: ":waveform" should be the command to toggle Waveform Tool Pane
- [ ] FIX: ":global-tool" waveform should be the command to toggle waveform global status
- [ ] When changing buffers in a pane, auto-resize the height of the relevant Tracks pane
- [ ] TODO: Deep dive on waveform ux and audio track syncing (auto, manual, & tapping)

### Chant

- [ ] Change drawing of neumes to be better looking
- [ ] TODO: Deep dive on chant staff representation

### File Save/Export

- [ ] Converting/exporting to chant (.gabc) files
- [ ] Nothing should be lost when writing to .muse files
- [ ] Some data may be lost when writing to .gp, .musexml, .gabc, or .txt

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Editing

- [ ] TODO: Deep dive on editing workflow
- [ ] Editing toolbar

### Keymaps

- [ ] TODO: Deep dive on keymap architecture and how to define (config.toml seems lacking)
  - Should be dependent on mode and currently focused window/pane/staf
- [ ] Different keymap for moving between staves
  - This allows moving to notes above or below standard staff without going to
    another staff

### Commands

- [ ] Change
  - close-pane -> pane-close (just rename)
  - quitall -> quit (quit should exit app)
  - write-quit (write all unsaved scores and quit app)

### UI

- [ ] FIX: Change _maximized_ indicator at right side of pane titlebar to  instead
      of just "Z"
- [ ] Properties pane (Score, Track, and Lyrics)
- [ ] Add scrollbar (side for vertical, bottom for horizontal)
  - [ ] Can be toggled on/off and changed in config.toml
- [ ] FIX: Remember last focused pane in tree so switching back to a row/column
      refocuses the last pane before focusing another row/column
- [ ] Allow grouping scores in tabline into groups and ability to (auto)
      collapse/expand them
- [ ] Add ability to have multiple windows in the same workspace (not just views
      and panes)
  - :workspace-close will close all windows attached to the workspace
  - :quit will close the app and all workspaces/windows
  - starting a new instance of muse will attach to the currently running
    workspace by default
  - can still start "muse --workspace NAME" to open a window in
    a new/different workspace

### TUI

- [ ] Implement some sort of rudimentary engraving. Start with tab+.

### GUI

- [ ] Try to closer emulate a GTK4 style

### Configuration

- [ ] Allow setting size and color of focus-ring
- [ ] Setting to enable/disable cycle/wrap focus of buffers in the tabline
- [ ] Ability to customize toolbars
- [ ] Ability to set corner radii

### Scripting

- [ ] Add mlua for ability to script application and create plugins
