# TODO

Planned work. Finished items move to `CHANGELOG.md` under `[Unreleased]`.

### Docs

- [ ] Static site for documentation (Zensical)
  - [ ] Github pages
- [ ] Document all options in docs/OPTIONS.md
  - Describe option types
  - How to set/get options
  - table of each option with type, default value, description, and example
- [ ] Document all commands in docs/COMMANDS.md
- [x] Document the specification for the .cc file in a section in docs/SPEC.md

### Refactor

- [x] Split chant and modern scores into 2 file extensions:
  - .ccc for cecilia chant
  - .ccm for cecilia modern

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

- [x] TODO: Deep dive on waveform ux and audio track syncing (auto, manual, & tapping)
- [ ] Add ability to create multitracks that are a combination of other tracks
- [ ] Toolbar
  - Window close button?
  - Reorder/regroup icons
  - Change icons

### Performance

- [ ] Compare CPU & mem usage to GP8 and MuseScoreStudio and Go Playalong

1. Redraw only the playhead and cursor area, or cache the engraved scene and just move the playhead overlay each frame. This is the main CPU fix.
2. Redraw only when the playhead has moved by at least a pixel or so, and lower the rate to 30 fps.
3. Measure memory first (e.g. with heaptrack, or by running with and without soundfont set) before changing anything.

### Chant

- [ ] Automatic audio sync for chant (`:sync-detect` in a chant score): track the recording's
      pitch (YIN, `cecilia-audio/src/tuner.rs`, ~10 ms hops), align the chant's note pitches and
      lengths by DTW allowing a constant transposition, set a point at each syllable start snapped
      to onsets, marked as detected (phase 3c step 8).
- [ ] Note ids through GABC text editing, properly (phase 4 / plan step 5): the properties pane's
      text edit and `:gabc` already match reparsed notes to the old ones by longest common
      subsequence on pitch and signs (`Chant::inherit_ids`); phase 4's splice edits should keep
      ids exactly, and the match could use the syllable text and shape too.

### File Save/Export

- [ ] Converting/exporting to chant (.gabc) files
- [ ] Nothing should be lost when writing to .ccc / .ccm files
- [ ] Some data may be lost when writing to .gp, .musexml, .gabc, or .txt

### File/Track Import

- [ ] Add ability to read/open a pdf or image of a score/tab/chant

### Audio

- [x] Add additional more realistic instrument sounds
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

- [x] Dim the voices not being edited in the GUI engraving (the terminal tab view does)
- [ ] Shortening or lengthening a beat in a voice should refill the rest of the bar with rests
      (or eat following rests), as Guitar Pro does; today the bar goes incomplete
- [ ] Voice-specific copy/paste of all voices of a selection at once (copy and paste work on the
      active voice), and voice stem-direction overrides per beat

### TUI

### GUI

- [ ] Try to more closely emulate a GTK4 style

### Scripting

- [ ] Add mlua for ability to script application and create plugins
- [ ] Add MCP server integration
