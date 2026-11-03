# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Engraving

- [ ] TODO: Clean up engraving bugs

### Tool Panes

- [ ] TODO: Deep dive on waveform ux and audio track syncing (auto, manual, & tapping)

### Chant

- [ ] Change the drawing of neumes and clef in the chant staff and chant files
      to be better looking like in this screenshot. i wan't the neumes to look less blocky.
- [ ] When i use a-j/k to move into the chant staff, the edit cursor should be
      able to highlight each position so that I can add/remove/change notes.
      right now, there is a highlight that goes across all staves.
- [ ] The representation of chant in the chant staff should match the output
      from the moder-to-chant command. including clivis, pes, bistropha,
      torculus, porrectus, climacus, etc.
- [ ] Since gregorian chant can't show chords, help me determine a good means of
      selecting the melody out of the track if any chords are present. or just fail to
      generate the chant staff with a message pointing to the illegal notation. Also
      help me decide how to integrate breath measures. I also want the track to
      play/sound the same regardless of the notation.
- [ ] TODO: Deep dive on chant staff representation

### File Save/Export

- [ ] Converting/exporting to chant (.gabc) files
- [ ] Nothing should be lost when writing to .muse files
- [ ] Some data may be lost when writing to .gp, .musexml, .gabc, or .txt

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Editing

- [ ] TODO: Deep dive on editing workflow

### Keymaps

- [ ] TODO: Deep dive on keymap architecture and how to define (config.toml seems lacking)
  - Should be dependent on mode and currently focused window/pane/staf
- [x] FIX: ctrl-c in any pane (score,tool,properties,etc.) should close that pane.

### UI

- [ ] Create a dashboard that is displayed on the background when all score
      panes are closed (also when opening a new workspace)
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
- [ ] Allow modifying the bar layout in "page layout". i.e. design mode
- [ ] Pane Resizing
  - Config setting for auto-resize
  - able to change via command
  - every pane should have a default w/h and current w/h
  - values for width/height can be fixed or dynamic (%)
  - :resize-default will set current w/h of the focused pane back to the default values
  - auto-resize should essentially run :resize-equal after opening or closing panes

### TUI

- [ ] FIX: there is no difference between layouts. horizontal still scrolls vertically
- [ ] Implement some sort of rudimentary engraving. Start with tab+.

### GUI

- [ ] Try to more closely emulate a GTK4 style

### Configuration

- [ ] Allow setting size and color of focus-ring
- [ ] Setting to enable/disable cycle/wrap focus of buffers in the tabline
- [ ] Ability to customize toolbars
- [ ] Ability to set corner radii

### Scripting

- [ ] Add mlua for ability to script application and create plugins
