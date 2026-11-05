# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Docs

- [ ] Static site for documentation (Zensical)
- [ ] Document all options in docs/OPTIONS.md
  - Describe option types
  - How to set/get options
  - table of each option with type, default value, description, and example
- [ ] Document all commands in docs/COMMANDS.md
- [x] Document the specification for the .cc file in a section in docs/SPEC.md

### Refactor

#### Options

- All settings are "options"
- Always in lower_snake_case
- Options are all defined via dot separation
  - e.g. tabline.truncation.max_width
- All options have a default value
- Remove all option-setting specific commands
- Options can be set from config.toml file directly
- Options can be set from command line with -o or --option
  - e.g. cecilia -o OPTION1=value1 OPTION2=value2.1,value2.2 OPTION3=true OPTION4=default
  - everything after "--" in command should be interpreted as file names
    - e.g. cecilia -o tabline.wrap=true -W foo -- song.cc tab.gp
- Options are set in app with command :set OPTION [value]
  - If no value, set to default value (can also be set with special value
    "default" to be explicit)
  - If invalid value is set, fail with helpful error message giving allowed values
  - Array of values option is set with a comma-separated list with no spaces
    like :set OPTION value1,value2,value3,...
- Print current value of option with :get OPTION

##### Option Types

- boolean
  - choices = true/false
  - special value "toggle" to switch
- integer or float
  - absolute values set with NUMBER
  - relative values set with +NUMBER or -NUMBER
- string
- array of values
  - add value to array with +value
  - remove value from array with -value

#### Commands

- Always in lower-kebab-case

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

### GUI

- [ ] Try to more closely emulate a GTK4 style

### Scripting

- [ ] Add mlua for ability to script application and create plugins
- [ ] Add MCP server integration
