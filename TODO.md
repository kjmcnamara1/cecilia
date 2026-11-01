# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Engraving

- [ ] TODO: Clean up engraving bugs

### Tool Panes

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

### Keymaps

- [ ] TODO: Deep dive on keymap architecture and how to define (config.toml seems lacking)
  - Should be dependent on mode and currently focused window/pane/staf
- [ ] Different keymap for moving between staves
  - This allows moving to add extra notes above or below standard staff without jumping to
    another staff
- [ ] add alt-j/k for focusing next/prev item in pickers and command mode.
      should do the same thing as tab/shift-tab.
- [ ] FIX: ctrl-c in any pane (score,tool,properties,etc.) should close that pane.
- [ ] ctrl-w ctrl-w cycles focus through panes
- [ ] ctrl-w ctrl-p switches focus back and forth between panes

### UI

- [ ] Create a dashboard that is displayed on the background when all score
      panes are closed (also when opening a new workspace)
- [ ] FIX: Remember last focused pane in tree so switching back to a row/column
      refocuses the last pane before focusing another row/column
  - e.g. 2 panes in left column, 1 pane in right. if bottom pane is selected
    in left, switching to right and back focuses the top pane in the left
    column, rather than the most recently focused bottom pane
  - e.g. 3 panes in left column, 1 pane in right. no matter which pane on
    the left is focused, when i switch to the right pane and back, the middle pane
    in the left column is always focused, not the last pane i had focused in that column
  - e.g. 2 panes in left column, 2 panes in right. moving in any direction
    focuses the pane next to it in that direction. this works correctly now.
  - i think the logic should work like this: when focusing in a certain
    direction and there are multiple panes that share the same edge with the
    current pane (i.e. touching), the most recently focused of those touching
    panes should be the one that now gets focused.
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
- [ ] FIX: ctrl-d/u should move the cursor vertically half a _screen_, not by
      bars.
  - Horizontal layout: total bars displayed on screen divided by 2.
  - Vertical and Page layout:
    - no horizontal movement of cursor
    - cursor stays on same line of staff
    - cursor jumps to [# of displayed combo staves] / 2. round up. minimum 1.
      - so if there are 3 combined staves displayed in the pane, move the
        cursor vertically down by 2 staves.
- [ ] Allow modifying the bar layout in "page layout". i.e. design mode
- [ ] Pane Resizing
  - Config setting for auto-resize
  - able to change via command
  - every pane should have a default w/h and current w/h
  - values for width/height can be fixed or dynamic (%)
  - :resize-default will set current w/h of the focused pane back to the default values
  - auto-resize should essentially run :resize-equal after opening or closing panes
- [ ] When closing a buffer with unsaved changes, prompt whether to save it or
      not first. don't just deny.
- [ ] When opening/creating a new score, it shouldn't be marked as
      edited/unsaved until something has been added. a new empty score should be able
      to be deleted buffer-closed without saving.

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
