# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Engraving

- [ ] TODO: Clean up engraving bugs

### Tool Panes

- [ ] TODO: Deep dive on waveform ux and audio track syncing (auto, manual, & tapping)

### Chant

- [ ] TODO: Deep dive on chant staff representation

### File Save/Export

- [ ] Converting/exporting to chant (.gabc) files
- [ ] Nothing should be lost when writing to .muse files
- [ ] Some data may be lost when writing to .gp, .musexml, .gabc, or .txt

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Editing

- [ ] TODO: editing workflow overhaul
  - the available editing tools on the palette and keybinds should depend on
    the current mode and the currently focused staff and notation
  - normal mode:
    - j: move cursor down to next string/line/space
    - k: move cursor up to next string/line/space
    - h: move cursor to previous beat (stay on same string/line/space)
    - l: move cursor to next beat (stay on same string/line/space)
    - w: move to next note (not tied) on current string (tab) or staff
      line/space (standard)
    - e: move to last note in tie or next note on string/line/space
    - b: move to previous note (beginning of tie) on string/line/space
    - ^: move to first note in bar (gh should do the same)
    - $: move to last note in bar (gl should do the same)
    - y: yank (copy)
      - yy: yank entire bar
    - p: paste after cursor
    - P: paste before cursor
    - x: delete single note under cursor
    - i: insert beat before cursor and switch to insert mode
    - a: insert beat after cursor and switch to insert mode
    - I: insert beat at beginning of bar and switch to insert mode
    - A: insert beat at end of bar and switch to insert mode
    - o: insert bar after current bar and switch to insert mode
    - O: insert bar before current bar and switch to insert mode
    - c: clear/change (delete and insert)
      - cc: clears bar and switch to insert mode
      - cl: clear beat under cursor and switch to insert mode (with rest)
      - ch: clear beat before cursor and switch to insert mode (with rest)
    - d: delete
      - dd: delete entire bar
      - dl: delete beat under cursor
      - dh: delete beat before cursor
    - v: visual beat mode selects entire beat
    - V: visual bar mode selects entire bar
  - visual mode:
    - i: insert beat before selection and switch to insert mode
    - a: insert beat after selection and switch to insert mode
  - insert mode:
    - enter/return: complete enter of fret # in tab or create note at cursor in other
      notation and move cursor to the next beat. auto-jump to beginning of next
      bar if cursor is on last complete beat of current bar. use duration of
      currently selected duration tool in palette
    - h/j/k/l: still move cursor
    - H: hammer-on/pull-off
    - L: let ring

- Text Editor Analogy
  - character = beat
  - word = note/sound
  - line = bar
  - paragraph = bar

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
