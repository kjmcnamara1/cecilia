# Options

Every setting of `config.toml` except the keymaps and `theme.cursor_border` is an *option*: a value with a type, a default
and a dotted name (`ui.tab_width` is the key `tab_width` in the `[ui]` table). Keymaps are not
options; see [keymap.md](keymap.md). [config.example.toml](config.example.toml) is a commented
config file to copy.

## Types

| Type | Accepts | Notes |
|---|---|---|
| bool | `true`, `false`, `toggle` | `toggle` flips the current value (`:set` and `-o` only). |
| integer *min*–*max* | a whole number | `+N` / `-N` adds to or subtracts from the current value. |
| number *min*–*max* | a decimal number | `+N` / `-N` likewise. |
| string | any text | An empty string is the same as unset for paths and fonts. |
| choice | one of the listed words | Exactly as written, lower case. |
| color | `#rgb`, `#rrggbb`, `#rrggbbaa`, or empty | Empty: use the preset's (or the current mode's) color. |
| list of … | comma-separated items | `+item` adds, `-item` removes (`:set ui.global_tools +tuner`); a plain list replaces it. |

A value outside a number's range is refused by `:set` and `-o`. In the config file it is clamped,
and any other invalid value is reset to the default, with a warning.

## Setting and getting options

**Config file** (`$XDG_CONFIG_HOME/cecilia/config.toml`, or `--config FILE`): the option's table
and key. Changes apply while cecilia runs (or run `:source`).

```toml
[ui]
tab_width = 4
global_tools = ["tracks", "waveform"]

[tabline.truncation]
max_width = 40
```

**Command line**: `-o NAME=VALUE` (repeatable, several may follow one `-o`; put `--` before file
names). Command-line values last for the session and win over the config file.

```sh
cecilia -o ui.tab_width=4 tabline.wrap=false -- song.gp5
```

**In the app**: `:set NAME VALUE` changes an option for this session (it is not written to the
config file) and wins over the file until you change it again. `:set NAME` or `:set NAME default`
resets it to the default; `:get NAME` shows the current value. Tab completes names and values.
Mistyped names suggest near matches.

```
:set ui.follow_playback toggle
:set ui.tab_width +1
:set ui.global_tools +tuner
:get ui.tab_width
:set ui.tab_width default
```

Keymaps can bind these too, e.g. `"<leader>f" = "set ui.follow_playback toggle"`.

Options marked *GUI* only affect the window, *TUI* only the terminal UI. Colors in the
tables below that read _preset_ come from the theme (`theme.preset`).

## Option reference

### `[general]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `general.leader` | string | `<Space>` | The key `<leader>` stands for in keymaps. | `<Space>` |

### `[ui]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `ui.tab_width` | integer 1–8 | `2` | TUI: characters per sixteenth note in the tab view. | `:set ui.tab_width +1` |
| `ui.follow_playback` | bool | `true` | Scroll to keep the playhead in view while playing. | `:set ui.follow_playback toggle` |
| `ui.jk_moves_between_staves` | bool | `false` | j/k go on from the last row of a staff into the next staff. | `:set ui.jk_moves_between_staves toggle` |
| `ui.lyrics_select_cursor` | bool | `true` | Going to a chant score's lyrics area puts the caret on the cursor's note. | `:set ui.lyrics_select_cursor toggle` |
| `ui.completion_ignore_case` | bool | `true` | Command-line Tab completion ignores letter case. | `:set ui.completion_ignore_case toggle` |
| `ui.completion_fuzzy` | bool | `true` | Command-line completion matches the typed letters in order anywhere. | `:set ui.completion_fuzzy toggle` |
| `ui.picker_position` | integer 0–100 | `25` | How far down the window (percent) the command palette and pickers sit. | `-o ui.picker_position=50` |
| `ui.picker_max_items` | integer 1–50 | `12` | The most candidates the command palette and pickers list at once. | `-o ui.picker_max_items=10` |
| `ui.frecency` | bool | `true` | Order command completion and the buffer, recent file and workspace pickers by how often and how lately each was used. | `:set ui.frecency toggle` |
| `ui.command_history_size` | integer 0–10000 | `500` | Command lines kept in the command history (0: no history). | `-o ui.command_history_size=10` |
| `ui.command_history_time` | choice: `relative`, `absolute` | `relative` | The command history shows when a line was run as how long ago, or as a date and time. | `:set ui.command_history_time absolute` |
| `ui.progress_bar` | bool | `true` | A progress bar along the bottom of every score pane. | `:set ui.progress_bar toggle` |
| `ui.scrollbar` | bool | `true` | A scrollbar on each score pane. | `:set ui.scrollbar toggle` |
| `ui.tracks_panel` | bool | `false` | Start with the tracks panel (tracks with mixer and bar overview) open. | `:set ui.tracks_panel toggle` |
| `ui.properties` | bool | `false` | Start with the properties pane (score, tracks and lyrics) open. | `:set ui.properties toggle` |
| `ui.palette` | bool | `false` | Start with the editing palette open. | `:set ui.palette toggle` |
| `ui.palette_wrap` | bool | `true` | The palette's keyboard motions wrap around at its edges. | `:set ui.palette_wrap toggle` |
| `ui.global_tools` | list of choices: `tracks`, `waveform`, `fretboard`, `piano`, `tuner` | `tracks,waveform,fretboard,piano,tuner` | Tool panes that dock at the full width of the view and follow the last score pane. | `:set ui.global_tools +tuner` |
| `ui.tracks_peek` | number 0–60 | `1` | Seconds the tracks show after a track change while no tracks pane is shown (0: off). | `-o ui.tracks_peek=10` |
| `ui.auto_resize` | bool | `true` | Tracks panes follow the height their score needs, until resized by hand. | `:set ui.auto_resize toggle` |
| `ui.equalize_on_close` | bool | `true` | Closing a pane gives the remaining panes of its split equal weights. | `:set ui.equalize_on_close toggle` |
| `ui.keep_files_in_new_view` | bool | `true` | A new view starts with the current files (off: the dashboard). | `:set ui.keep_files_in_new_view toggle` |
| `ui.workspace_autosave_on_quit` | bool | `true` | Save the active workspace when quitting; :quit! never does. | `:set ui.workspace_autosave_on_quit toggle` |
| `ui.workspace_restore_last` | bool | `true` | Starting without arguments loads the last workspace. | `:set ui.workspace_restore_last toggle` |
| `ui.toolbar` | bool | `true` | Start with the toolbar of common commands at the top of the window. | `:set ui.toolbar toggle` |
| `ui.fretboard` | bool | `false` | Start with the fretboard panel open. | `:set ui.fretboard toggle` |
| `ui.piano` | bool | `false` | Start with the piano panel open. | `:set ui.piano toggle` |
| `ui.hide_cursor_while_following` | bool | `true` | While playing with follow on, hide the edit cursor. | `:set ui.hide_cursor_while_following toggle` |
| `ui.layout` | choice: `page`, `vertical`, `horizontal` | `vertical` | Default GUI score layout for new panes. | `:set ui.layout page` |
| `ui.zoom` | number 0.1–10 | `1` | Default GUI zoom factor at startup. | `:set ui.zoom 1.5` |
| `ui.chant_chords` | choice: `top`, `bottom`, `error` | `top` | Which note of a chord :modern-to-chant keeps. | `:set ui.chant_chords error` |
| `ui.bar_numbers` | choice: `system`, `every` | `system` | Bar numbers on the score: the first bar of each line, or every bar. | `:set ui.bar_numbers every` |
| `ui.tab_rhythm` | choice: `auto`, `on`, `off` | `auto` | Rhythm row under tab: always, never, or only when the track has no standard or slash staff. | `:set ui.tab_rhythm off` |
| `ui.highlight_incomplete_bars` | bool | `false` | Draw the staff lines of bars that don't hold their full count in red. | `:set ui.highlight_incomplete_bars toggle` |
| `ui.focus_ring_width` | number 0–8 | `4` | Thickness in pixels of the ring around the focused pane in the GUI (0: none). | `-o ui.focus_ring_width=2` |
| `ui.focus_ring_color` | color | _unset_ | Color of the focus ring; empty: the current mode's color. | `:set ui.focus_ring_color #fabe28` |
| `ui.lyrics_font` | string | `Source Sans 3` | GUI: the font family of the lyrics text area in the properties pane. | `-o "ui.lyrics_font=Noto Sans"` |
| `ui.lyrics_font_size` | number 4–96 | `12` | GUI: the lyrics text area's font size in points. | `-o ui.lyrics_font_size=10` |
| `ui.chant_text_font` | choice: `noto-serif`, `source-sans` | `noto-serif` | The font of the lyrics of a chant score. | `:set ui.chant_text_font source-sans` |
| `ui.mark_labels` | choice: `exact`, `above`, `off` | `exact` | Where the marks are drawn on the score while ' waits for a letter: on the exact position, above the staff, or not at all. | `:set ui.mark_labels off` |
| `ui.renderer` | choice: `auto`, `gpu`, `software`, `cpu` | `auto` | GUI only: the renderer. auto tries the GPU, then software, then cpu; cpu renders without any graphics API (slower), for machines with no GPU or driver; takes effect at the next start (CECILIA_RENDERER overrides it). | `-o ui.renderer=software` |
| `ui.auto_update` | choice: `on`, `ask`, `notify`, `off` | `notify` | A newer release, looked for at most once a day at startup: on downloads it and installs it when cecilia exits, ask asks first, notify only says so (and how to update), off never looks. Only the Windows installer's install updates itself; others treat on and ask as notify. | `:set ui.auto_update off` |
| `ui.waveform_time_ticks` | choice: `all`, `major`, `off` | `all` | GUI: which ticks of the waveform's time ruler are drawn. | `:set ui.waveform_time_ticks off` |
| `ui.waveform_beat_ticks` | choice: `all`, `major`, `off` | `all` | GUI: which ticks of the waveform's score ruler are drawn. | `:set ui.waveform_beat_ticks off` |
| `ui.waveform_time_labels` | bool | `true` | GUI: label the major ticks of the time ruler. | `:set ui.waveform_time_labels toggle` |
| `ui.waveform_beat_labels` | bool | `true` | GUI: label the ticks of the score ruler. | `:set ui.waveform_beat_labels toggle` |
| `ui.sync_snap` | number 0–200 | `70` | Sync mode: milliseconds a sync point may move to the strongest nearby onset (0: none). | `-o ui.sync_snap=10` |

### `[tabline]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `tabline.new_view` | choice: `copy`, `current`, `empty` | `copy` | What a new view starts with: the current buffers, the current buffer or nothing. | `:set tabline.new_view empty` |
| `tabline.icons` | choice: `auto`, `nerd`, `unicode`, `none` | `auto` | File-type icons in the terminal UI (auto: Nerd Font if detected, else Unicode, else ASCII). | `:set tabline.icons none` |
| `tabline.hide_single_view` | bool | `true` | Hide the views on the right of the tabline while there is only one. | `:set tabline.hide_single_view toggle` |
| `tabline.wrap` | bool | `true` | Buffer next/prev wrap around at the ends of the tabline. | `:set tabline.wrap toggle` |

### `[tabline.truncation]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `tabline.truncation.max_width` | integer 1–200 | `28` | Longest file name (characters) a tab shows before truncating. | `-o tabline.truncation.max_width=10` |
| `tabline.truncation.prefix_width` | integer 0–200 | `14` | Characters kept from the start of a truncated tab name. | `-o tabline.truncation.prefix_width=10` |
| `tabline.truncation.suffix_width` | integer 0–200 | `10` | Characters kept from the end of a truncated tab name. | `-o tabline.truncation.suffix_width=10` |

### `[audio]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `audio.soundfont` | string | _unset_ | SoundFont (.sf2) for MIDI playback; empty: the built-in synth. | `audio.soundfont = "~/sf/GeneralUser-GS.sf2"` |
| `audio.speed` | number 0.1–4 | `1` | Playback speed multiplier (1.0 = written tempo). | `:set audio.speed 0.75` |
| `audio.tuner_a4` | number 380–500 | `440` | The tuner's reference pitch for A4, in Hz. | `:set audio.tuner_a4 442` |

### `[swap]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `swap.enabled` | bool | `true` | Write swap files (recovery copies of unsaved buffers). | `:set swap.enabled toggle` |
| `swap.interval` | number 0.1–3600 | `5` | Seconds after the last change before the swap file is written. | `-o swap.interval=10` |
| `swap.auto_recover` | bool | `true` | Opening a file with a newer swap file restores it as unsaved changes. | `:set swap.auto_recover toggle` |
| `swap.dir` | string | _unset_ | Where swap files go; empty: the XDG state directory. | `swap.dir = "~/.cache/cecilia/swap"` |

### `[notifications]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `notifications.anchor` | choice: `top_left`, `top`, `top_right`, `right`, `bottom_right`, `bottom`, `bottom_left`, `left` | `top_right` | The corner or side notification cards stack against. | `:set notifications.anchor top_left` |
| `notifications.area` | choice: `scores`, `window` | `scores` | What the cards sit in: the area of the score panes, or the whole window. | `:set notifications.area window` |
| `notifications.info_timeout` | number 0–3600 | `4` | Seconds an information card stays (0: until dismissed). | `-o notifications.info_timeout=10` |
| `notifications.error_timeout` | number 0–3600 | `8` | Seconds an error card stays (0: until dismissed). | `-o notifications.error_timeout=10` |
| `notifications.max_visible` | integer 1–20 | `5` | Most cards shown at once; the rest are counted in a "+N more" line. | `-o notifications.max_visible=10` |
| `notifications.history_size` | integer 0–10000 | `500` | Notifications kept in the history picker (0: no history). | `-o notifications.history_size=10` |
| `notifications.history_time` | choice: `relative`, `absolute` | `relative` | The history picker shows when a notification appeared as how long ago, or as a date and time. | `:set notifications.history_time absolute` |

### `[popups]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `popups.opacity` | number 0–100 | `85` | GUI: how opaque (percent) pickers, the palette and notification cards are (100: solid). | `-o popups.opacity=10` |
| `popups.blur` | number 0–100 | `20` | GUI: how far the page behind pickers, the palette and notification cards is blurred, in pixels (0: not blurred). | `-o popups.blur=10` |
| `popups.shadow_opacity` | number 0–100 | `45` | GUI: how dark (percent) the shadow under popups is (0: none). | `-o popups.shadow_opacity=10` |
| `popups.shadow_blur` | number 0–80 | `15` | GUI: how far the shadow under popups is blurred, in pixels. | `-o popups.shadow_blur=10` |
| `popups.shadow_offset` | number 0–40 | `7` | GUI: how far the shadow under popups is pushed down, in pixels. | `-o popups.shadow_offset=10` |

### `[theme]`

The colors are listed [below](#theme-colors).

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `theme.preset` | string | `dark` | The color theme: a built-in one or a file in the themes folder of the config folder. | `:set theme.preset nord` |
| `theme.file` | string | _unset_ | A theme file with the same keys, applied under the colors set here. | `theme.file = "themes/mine.toml"` |
| `theme.track_colors` | list of colors | _unset_ | The colors of tracks 1, 2, ...; empty: the built-in palette. | `:set theme.track_colors #e05050,#4090e0` |
| `theme.voice_colors` | list of colors | _unset_ | The colors of voices 1, 2, ...; empty: the built-in palette. | `:set theme.voice_colors +#aa64d2` |
| `theme.selection_opacity` | number 0–100 | _preset_ | How opaque the Visual-mode selection is, in percent; default: the preset's. | `:set theme.selection_opacity 35` |
| `theme.cursor_opacity` | number 0–100 | _preset_ | How opaque the edit cursor's shade is, in percent; default: the preset's. | `:set theme.cursor_opacity 60` |

### `[toolbar]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `toolbar.items` | list of strings | `new,open,write,\|,undo,redo,\|,play-toggle,stop,set ui.follow_playback toggle,loop-toggle,metronome,count-in,\|,tracks,properties,waveform,piano,fretboard,tuner,split,vsplit,\|,command-mode,pick-keymap` | The toolbar's buttons: command lines in order, "\|" between groups. | `:set toolbar.items +split` |

### `[statusline]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `statusline.separators` | choice: `arrow`, `round`, `none` | `arrow` | TUI: how the status line separates its sections. | `:set statusline.separators none` |

### `[score]`

| Option | Type | Default | Description | Example |
|---|---|---|---|---|
| `score.clef` | choice: `treble`, `bass`, `grand` | `treble` | Clef of a standard staff added without one. | `:set score.clef grand` |

### Theme colors

Each is a *color* option, `theme.NAME`. Unset, a color comes from the preset; the value shown is
the `dark` preset's. Setting one overrides the preset (e.g. `:set theme.accent #ff8800`).

| Option | Default (`dark`) |
|---|---|
| `theme.paper` | `#e8e9ec` |
| `theme.page` | `#ffffff` |
| `theme.ink` | `#000000` |
| `theme.chrome` | `#282a30` |
| `theme.chrome_text` | `#e1e1e6` |
| `theme.dim` | `#9698a0` |
| `theme.accent` | `#fabe28` |
| `theme.playhead` | `#e12828` |
| `theme.selection` | `#965adc` |
| `theme.tab` | `#34373f` |
| `theme.tab_hover` | `#484b56` |
| `theme.status_b` | `#3c3f49` |
| `theme.toolbar` | `#2e3037` |
| `theme.button_hover` | `#484b56` |
| `theme.capo` | `#e1e1e6` |
| `theme.panel` | `#1e2025` |
| `theme.empty_bar` | `#373a42` |
| `theme.incomplete_bar` | `#d62828` |
| `theme.popover` | `#24262c` |
| `theme.error` | `#ff6e64` |
| `theme.lyrics` | `#6ec882` |
| `theme.marker` | `#5abed2` |
| `theme.lyrics_text` | `#e1e1e6` |
| `theme.lyrics_text_dim` | `#9698a0` |
| `theme.lyrics_gabc` | `#f0f0f4` |
| `theme.lyrics_label` | `#64a0f0` |
| `theme.lyrics_anchor` | `#f05a5a` |
| `theme.lyrics_clef` | `#be82e6` |
| `theme.lyrics_division` | `#f05a5a` |
| `theme.lyrics_note` | `#64a0f0` |
| `theme.lyrics_shape` | `#e6c85a` |
| `theme.lyrics_attach` | `#5ac8d2` |
| `theme.lyrics_space` | `#828690` |
| `theme.lyrics_markup` | `#6ec882` |
| `theme.waveform` | `#6eaae6` |
| `theme.loop_region` | `#3cbe6e32` |
| `theme.sync_point` | `#fabe28` |
| `theme.mode_normal` | `#5a96e6` |
| `theme.mode_insert` | `#64be78` |
| `theme.mode_visual` | `#be78dc` |
| `theme.mode_command` | `#f0b43c` |
| `theme.mode_picker` | `#50bec8` |
| `theme.mode_tracks` | `#e67850` |
| `theme.icon_guitar_pro` | `#f08c32` |
| `theme.icon_musicxml` | `#5096e6` |
| `theme.icon_midi` | `#5abe6e` |
| `theme.icon_chant` | `#b46ed2` |
| `theme.icon_cecilia` | `#3cbebe` |
| `theme.score_modern` | `#d68c50` |

`theme.cursor_border` (the outline of the edit cursor; default: near-black `#141418`, or a color that
contrasts with the page) is a config-file-only key of `[theme]` or a theme file. It is not an
option, so `:set`, `:get` and `-o` do not know it.
