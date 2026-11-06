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

### Lyrics

- toggle switch above textarea to show/edit the raw gabc
  - if on:
    - display and allow editing everything in parentheses (gabc) as well as the actual lyric text
    - (gabc) text should be bold foreground color font
    - [Verse labels] text should be bold blue font
    - <bar:beat> anchor text should be red font
    - Lyric text should be normal but dimmed foreground color font
    - all these colors can be set via config.toml and every built-in theme
      should provide them and they should fit the colorscheme
- textarea should contain all the lyrics for the entire track in gabc lang (single source of truth)
  - there should be no "Lyrics" label
  - there should be no textarea background or border
  - when the "cursor" moves down from the gabc toggle, the normal mode
    neovim-like block text cursor will appear
  - when in normal mode in text area, "enter" will function just like "i"
  - pressing "esc" when in textarea normal mode will defocus the entire
    properties pane
  - can focus the gabc toggle with ctrl-k and refocus the textarea with ctrl-j
    when in textarea normal mode
  - theme should be able to set font colors for textarea
  - font should be monospace in textarea
  - should be able to set font family (default JetBrainsMono) and height (default 14pt)
- text inside () is parsed as gabc and will be used only for chant notation on a staff
- newlines are not parsed
- whitespace in excess of 1 space is ignored
- "*" as a standalone word is rendered as  (six pointed asterisk), but is not attached as a syllable
- word broken by "-" splits syllables with a dash
- special "words"
  - a word of all "-" is rendered as a single dash. the length determines how many beats the dash covers
  - a word of all "_" is not rendered. the length determines how many beats are not rendered
- syllables are assigned to beats in order following the progression of the score including repeats, alternate endings, and directions
- text inside [] is displayed on the score, but not attached as a syllable. this allows displaying verse numbers
- lyrics anchor points can be set with <bar:beat>. otherwise lyrics start at the first note/beat in the score (including pickup).
- the :lyrics command inserts an anchor tag into the lyrics pane before the words
- lyrics entered with the :lyrics command follow the same rules. newlines can be
  entered with \n

e.g.

- "someone" is 1 beat
- "some one" and "some_one" are 2 beats separated by space
- "some-one", "some--one", "some- one", and "some -one" are all 2 beats separated by a dash
- "some - one" is 3 beats separated by a dash
- "some -- one" is 4 beats separated by a dash
- "some _ one" is 3 beats separated by space
- "some __ one" is 4 beats separated by space

e.g. lyrics =
[1.] Let all mor-tal flesh keep _ si-lence,
And with fear and trem-bling _ stand;

[2.] Rank on rank the host of _ hea-ven
Spreads its van-guard on the _ way,

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

### Voices

- [ ] Dim the voices not being edited in the GUI engraving (the terminal tab view does)
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
