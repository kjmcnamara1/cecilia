# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Docs

- [ ] Static site for documentation (Zensical)

### Tool Panes

- [ ] TODO: Deep dive on waveform ux and audio track syncing (auto, manual, & tapping)

### Chant

- [ ] TODO: Deep dive on chant staff representation

### File Save/Export

- [ ] Converting/exporting to chant (.gabc) files
- [ ] Nothing should be lost when writing to .cc files
- [ ] Some data may be lost when writing to .gp, .musexml, .gabc, or .txt

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Audio

- [ ] Add additional more realistic instrument sounds
- [ ] Add AI singing voice that can sing lyrics?

### Keymaps

- [ ] TODO: Deep dive on keymap architecture and how to define (config.toml seems lacking)
  - Should be dependent on mode and currently focused window/pane/staff

### UI

- [ ] Create a dashboard that is displayed on the background when all score
      panes are closed (also when opening a new workspace)
  - :pane-close shouldn't quit the app
  - Focusable buttons with keyboard and clickable with mouse:
    - Select Workspace (opens workspace picker)
    - New Score
    - Open Score
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

### TUI

- [ ] FIX: there is no difference between layouts. horizontal still scrolls vertically

### GUI

- [ ] Try to more closely emulate a GTK4 style
- [ ] FIX: soften the shadow under the command palette/pickers

### Scripting

- [ ] Add mlua for ability to script application and create plugins
- [ ] Add MCP server integration
