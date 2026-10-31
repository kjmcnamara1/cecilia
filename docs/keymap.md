# Default keymap

All bindings can be overridden in `config.toml` under `[keymap.<mode>]`. `<leader>` defaults to `<Space>`.
Items marked _(M1)_ are implemented in the first milestone; the rest are the planned defaults.

## Normal mode

### Cursor movement

| keys                      | command                                                                                                                                                  |        |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `h` / `l`                 | previous / next beat                                                                                                                                     | _(M1)_ |
| `j` / `k`                 | next / previous row: tab strings and staff lines and spaces, on through the staves of the part (down from a staff's last row is the next staff's first) | _(M1)_ |
| `w` / `b`                 | next / previous beat with notes                                                                                                                          | _(M1)_ |
| `}` / `{`                 | next / previous bar                                                                                                                                      | _(M1)_ |
| `0` / `$`                 | first / last beat of bar                                                                                                                                 | _(M1)_ |
| `gg` / `G`                | first bar / last bar (`[count]G` = bar _count_)                                                                                                          | _(M1)_ |
| `J` / `K`                 | next / previous track                                                                                                                                    | _(M1)_ |
| `]s` / `[s`         | next / previous section (`[count]` sections away; `''` goes back) |        |
| `<leader>S`         | sections picker: every section of the file with its bars; Enter jumps there (`:sections`) | |
| `m{a-z}` / `m{A-Z}`       | set a mark in this file / a global mark (any file)                                                                                                       |        |
| `'{mark}` / `` `{mark} `` | jump to the mark's bar / exact position (a popup lists the marks and where they are); a global mark reopens its file if it was closed. `''` jumps back to before the last jump (also after `gg`, `G`) |        |
| `<C-d>` / `<C-u>`         | half page down / up                                                                                                                                      | _(M1)_ |

### Playback

| keys                        | command                                                                                                                                                                                                                 |        |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `<leader><leader>`          | play / pause from cursor                                                                                                                                                                                                | _(M1)_ |
| `<leader>s`                 | stop                                                                                                                                                                                                                    | _(M1)_ |
| `<leader>f`                 | toggle following playback. A red bar always marks the playhead; while following, the editing cursor and the view move with it. Turned off (even mid-playback), the cursor stays where it is and is moved only by `hjkl` | _(M1)_ |
| `<leader>l`                 | loop the selected bars (or the cursor bar) / loop off                                                                                                                                                                   | _(M5)_ |
| `<leader>c`                 | toggle count-in                                                                                                                                                                                                         | _(M5)_ |
| `<leader>m`                 | toggle metronome                                                                                                                                                                                                        | _(M5)_ |
| `<leader>-` / `<leader>=`   | speed −5% / +5%                                                                                                                                                                                                         | _(M5)_ |
| `<leader>a`                 | cycle mix: audio + MIDI / audio only / MIDI only                                                                                                                                                                        | _(M5)_ |
| `<leader>t`                 | sync tap: place the next bar at the current audio time                                                                                                                                                                  | _(M5)_ |
| `<leader>w`                 | show / hide the waveform tool pane (`:waveform`) | _(M5)_ |
| `<leader>v`                 | cycle the focused pane's score layout (vertical / page / horizontal); new splits and views copy it                                                                                                                      | _(M3)_ |
| `<C-=>` / `<C-->` / `<C-0>` | zoom the focused pane in / out / reset (GUI); Ctrl+wheel zooms the pane under the mouse                                                                                                                                                                        | _(M3)_ |

### Buffers, panes, views

The model, as in Neovim with bufferline.nvim: a **pane** shows one **buffer** (an open file);
a **view** is a layout of panes with its own set of loaded buffers, shown as tabs on the left of
the tabline; the views are listed on the right of the tabline (only once there are two or more, unless `tabline.hide_single_view = false`); a **workspace** saves the views.
A file open in several views is one shared buffer (edits and undo show in all of them).

Each buffer tab shows a file-type icon, the name (shortened in the middle when longer than
`tabline.truncation.max_width`, keeping the first `prefix_width` and last `suffix_width`
characters), and on mouse hover an `×` to close it (`●` marks unsaved changes). Click a tab to
show it in the focused pane, click `×` or middle-click to close it, click a view to switch.

Closing a buffer's tab removes it from the view; panes showing it go back to the most recent
other buffer they showed (the view's only pane falls back to the neighbouring tab) and close if
there is none. The buffer is unloaded once no view has it; unsaved changes then stop the close
unless forced (`:bdelete!`).

| keys                | command                                   |        |
| ------------------- | ----------------------------------------- | ------ |
| `<leader>b`         | buffer picker                             |        |
| `<leader>V`         | view picker                               |        |
| `<leader>W`         | workspace picker                          |        |
| `L` / `H`           | next / previous buffer tab                |        |
| `<leader>x`         | close the buffer's tab                    |        |
| `gt` / `gT`         | next / previous view                      |        |
| `<C-w>s` / `<C-w>v` | split horizontally / vertically           | _(M1)_ |
| `<C-w>h/j/k/l`      | focus pane left/down/up/right             | _(M1)_ |
| `<C-w>q`            | close pane (the last one closes the view) | _(M1)_ |
| `<C-w>+` / `<C-w>-` | pane taller / shorter by 5% (`:resize +N`, `:resize N` in percent of its split) | |
| `<C-w>>` / `<C-w><` | pane wider / narrower by 5% (`:vertical-resize`) | |
| `<C-w>=`            | all panes the same size (`:resize-equal`) | |
| `<C-w>H` `J` `K` `L` | move the pane to the far left / bottom / top / right, spanning the view (`:pane-move-left` …) | |
| `<C-w>x`            | swap the pane with the next one (`:pane-exchange`) | |
| `<C-w>r` / `<C-w>R` | rotate the panes of this split forwards / backwards (`:pane-rotate`, `:pane-rotate-back`) | |
| `<leader>>` / `<leader><` | move this file's tab right / left in the tabline (`:buffer-move-next`, `:buffer-move-prev`; `:buffer-move N`, `+N`, `-N`) | |
| `<A-m>` / `<C-w>z`  | maximize the pane / restore the others (`:maximize`); focusing another pane restores them too | |

Besides scores, a pane can show a **tool**: the tracks (`:tracks`), the audio waveform
(`:waveform`), the piano (`:piano`), the fretboard (`:fretboard`) or the tuner
(`:tuner`). Each command shows its tool, or hides it. A tool listed in `ui.global_tools` (all
of them by default; `:global-tool TOOL` switches one between global and local, moving it if it
is open) docks along the bottom of the whole view, above the status line, as wide as
the window, and shows the score pane worked on last. Any other tool opens at the bottom of the
score pane worked on last, as wide as it, and always shows that pane's score (focusing it
makes that pane the one worked on again); it closes with its score pane. `<Esc>` in a tool
pane goes back to the score pane worked on last. A tool
pane keeps its own size — the height (or width) its contents need, until you drag its border or
`:resize` it — while panes come and go; score panes share the room left, equally when split.
Tool panes are panes like the others: `<C-w>h/j/k/l` focus them, `<C-w>H/J/K/L`
dock them along any edge, and they swap, rotate, resize, maximize and close (`:pane-close`) like score
panes. They work on the view's score pane — the one focused last — and editing keys typed in
a tool pane go to it. Closing the last score pane closes the view.

Drag a tab along the tabline to reorder the files. Drag the border between two panes with the
mouse to resize them, and the top edge of a pane's waveform strip to make it taller or shorter
(`:tracks-resize N` resizes the tracks pane, `:waveform-resize N` sets the strips' height in
pixels). Panes, their sizes and the waveform height are saved in workspaces. Over the tracks pane the mouse wheel
scrolls through tracks, and sideways (or with Shift) through bars; over a waveform both wheels
move along the timeline and Ctrl+wheel zooms it. They follow the cursor or playhead again once
that moves.

**Pickers** open in the command palette's box and list the view's buffers (with their folder,
● when modified), the views (with their files) or the saved workspaces (with their view and file
counts). Typing filters the list with the same matching as completion (fuzzy, case-insensitive
by default) and highlights what matched; the top match is selected — `<Up>`/`<Down>` (or
`<Tab>`/`<S-Tab>`, `<C-p>`/`<C-n>`) move, `<Enter>` switches, `<Esc>` cancels. The buffer and
view pickers also work while a song plays.

| command                                                                                          |                                                                                                     |
| ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------- |
| `:open {file}`                                                                                   | open a file in the focused pane, adding it to the view's tabs                                       |
| `:open` (`:open-dialog`, `<C-o>`) | choose one or more files in the system file picker; they open as tabs and the first is shown |
| `<leader>?` (`:keymaps`, `:pick-keymap`) | every key binding of every mode, found by keys, command or description; Enter runs a Normal-mode one |
| `:pick-buffer` / `:pick-view` / `:pick-workspace` (`:pick buffer\|view\|workspace\|mark\|macro`) | open a picker                                                                                       |
| `:marks` / `:macros` (`:registers`)                                                              | pick a mark to jump to / a macro to run                                                             |
| `:mark x` (`:k x`) / `:delmarks xyz` / `:delmarks!`                                              | set a mark / delete marks / delete this file's lowercase marks                                      |
| `:goto-mark x` / `:run-macro r`                                                                  | jump to a mark / run a macro (for bindings)                                                         |
| `:bnext` / `:bprev` / `:buffer N`                                                                | next / previous / Nth buffer tab                                                                    |
| `:bdelete [N]` / `:bdelete! [N]`                                                                 | close the focused (or Nth) buffer tab / discarding unsaved changes                                  |
| `:view-new [file]`                                                                               | new view: a copy of the current view's tabs, just the current buffer, or empty (`tabline.new_view`) |
| `:view-close` / `:view-close!`                                                                   | close the view (buffers only it had are unloaded)                                                   |
| `:view N` / `:view-next` / `:view-prev`                                                          | switch views                                                                                        |
| `:view-rename [name]`                                                                            | name the view in the tabline (no name: its number)                                                  |

### Editing (M4, done)

| keys                                                   | command                                                                                           |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `i` / `a`                                              | insert at the cursor beat / append a beat and insert                                              |
| `x`                                                    | delete the note on the cursor string (`[count]x`)                                                 |
| `d{motion}` / `dd`                                     | delete beats / bars (`[count]dd`)                                                                 |
| `y{motion}` / `yy`, `p` / `P`                          | yank beats or bars / put after / before                                                           |
| `c{motion}` / `cc`                                     | change (delete, then insert)                                                                      |
| `"{reg}`                                               | use register for the next yank/delete/put                                                         |
| `.`                                                    | repeat the last change (including an insert session)                                              |
| `u` / `U` / `g-` / `g+`                                | undo / redo / older / newer state across undo branches                                            |
| `q{reg}` … `q` / `[count]@{reg}` / `@@`                | record / play / replay a macro; `q{A-Z}` appends to the lowercase register                        |
| `v` / `V`                                              | visual beats / visual bars (then `d`, `y`, `c`, `x`)                                              |
| `o` / `O`                                              | new bar after / before                                                                            |
| `+` / `-`                                              | longer / shorter note value                                                                       |
| `<C-a>` / `<C-x>`                                      | pitch up / down a semitone (moves string when the fret runs out)                                  |
| `r`                                                    | make the beat a rest                                                                              |
| `th` `tb` `ts` `tv` `tx` `tm` `tl` `tg` `ta` `tn` `tt` | toggle hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| `t.` / `t3`                                            | toggle dot / triplet                                                                              |
| `ZZ`                                                   | write and close                                                                                   |

### Other

| keys        | command          |        |
| ----------- | ---------------- | ------ |
| `:`         | command line     | _(M1)_ |
| `<F1>`      | help             |        |
| `<leader>?` | find a key binding (keymaps picker) |        |
| `ZZ`        | write and quit   |        |
| `<C-q>`     | quit muse (`:quit`; not while files have unsaved changes) | |

## Status line

As lualine.nvim: on the left the mode (in its color), the pane's layout (in the mode's color),
the workspace and the file (● when changed); on the right the tempo, play time, play-along
settings, macro recording, insert note length and zoom, then the position as `Top` / `NN%` /
`Bot` and `bar:beat` in the mode's color. Click the layout to cycle it, the workspace to pick
another, the mix icon (󰣱 synth, 󰝚 audio, 󰣱 +󰝚 both) to cycle what plays, and the speed (󰓅) or
zoom (shown only when not 100%) to reset it to 100%. The GUI draws colored sections as rounded chips;
in the terminal `[statusline] separators` chooses `arrow` (default), `round` or `none` between
them (Nerd Font glyphs when `tabline.icons = "nerd"`, flat blocks otherwise). `theme.status_b`
colors the second sections. The tuning and capo of a fretted track are printed under the
track name at the top of the score (and in the pane title in the terminal and in the GUI's
horizontal layout).

## Toolbar

At the top of the window, above the tabline, a toolbar has buttons for common commands: new, open, save, undo, redo,
play/pause, stop, follow, loop, metronome, count-in, tracks panel, splits, the command line and
the keymaps picker. Toggles light up while on; buttons with nothing to do are dimmed. Resting
the mouse on a button shows what it does and its key (in the terminal, on the command line).
`<leader>B` or `:toolbar [on|off]` shows or hides it (`ui.toolbar` sets the default; workspaces
remember it), and `[toolbar] items` in config.toml chooses the buttons: any command line, with
`"|"` between groups. The terminal draws its icons in the `tabline.icons` style (`none`: words).

## Themes

`[theme]` in config.toml picks a built-in theme (`preset`: dark, light, nord, gruvbox,
solarized-dark, solarized-light), an optional theme file of your own (`file`), and any color by
name: the window chrome, toolbar, tabs, panels, palette, cursor, playhead, waveforms, mode
labels, file icons, the track palette, and the score's page and ink — set a dark `page` and a
light `ink` for light notation on dark paper. `docs/config.example.toml` lists every name.

## Fretboard and piano

`<leader>F` (`:fretboard [on|off]`) shows a fretboard pane and `<leader>K` (`:piano [on|off]`) a
piano pane, for the score pane's track: the notes at the edit cursor — or
under the playhead while playing — as dots on the neck (named, in the track's color; the
cursor's string in the mode color, a capo across its fret in `theme.capo`) and as pressed keys. The GUI's neck
spaces its frets as a real one, with inlays; its piano has all 88 keys. `ui.fretboard` and
`ui.piano` set the defaults, and workspaces remember them. Clicking a fret puts that note on
that string in the cursor beat (clicking it again takes it off); clicking a key adds that pitch
as a chord note on a free string that can play it (the cursor's string first), or toggles that
drum on a drum track.

## Tuner

`<leader>u` (`:tuner [on|off]`) shows a tuner pane and listens to the default
audio input (a microphone or an instrument input) while it is shown. It names the note it
hears, shows how many cents off it is on a −50…+50 scale (green within 3 cents, then yellow,
red past 15), the frequency, and the nearest open string of the focused track's tuning (capo
included) and how far from it. `audio.tuner_a4` sets the reference pitch (440 Hz).

## Tracks panel

`<leader>T` shows or hides the tracks pane (`ui.tracks_panel` shows it from the start;
workspaces remember it). While no tracks pane is shown, the tracks show along the bottom for a
second whenever a key or command changes the track (`ui.tracks_peek` sets the seconds; 0 turns
that off). Each track of the score pane's file has a row: its number, a
volume slider, **S**olo and **M**ute, its name and instrument, then a block per bar — colored
where the bar has notes, faded when the track isn't heard. The edit cursor is a black dot on
its bar's block and the red playhead runs across all tracks. Click a block to move the cursor there, click S or
M to toggle them, drag a slider to set the volume (one undo step). Double-click a track's name
to rename it in place, or its instrument to pick another.

Below the tracks come the **audio track** (when the file has one), drawn as a simplified
waveform under the bars, and the **synth master**, showing the section markers (a flag, or the
section's letter in a box, then its title). Their sliders set the audio level and the master
level, which scales the final output without changing any track's volume.

`<C-w>t` focuses the tracks pane (opening it; again, back to the score pane). It stays Normal
mode; while the pane has the focus its keys are `[keymap.tracks]`, then the usual ones:

| keys | command |
| --- | --- |
| `j` `k` / `<Down>` `<Up>` | select the next / previous row (tracks, then the audio track and the master); the pane keeps showing its track |
| `<CR>` | show the selected track in the pane |
| `h` `l` / `<Left>` `<Right>`, `gg` `G` | previous / next bar, first / last bar |
| `s` / `m` | solo / mute the selected row |
| `+` (`=`) / `-` | the selected row's volume up / down 5% |
| `c` | next color of the theme's palette for the selected track |
| `t` | rename the selected track in place: type the name, `<CR>` keeps it, `<Esc>` drops it |
| `i` | pick the selected track's instrument |
| `<Esc>` `q` | back to the score pane |
| `<leader><leader>` `<leader>s` `<leader>f` `:` | play/pause, stop, follow, command line |

| command | |
| --- | --- |
| `:tracks [on\|off]` / `:tracks-focus` | show or hide the tracks pane / focus it (again: back to the score pane) |
| `:track-rename NAME [n]` | rename track n (default: the cursor's track) |
| `:track-instrument [PROGRAM\|drums] [n]` | set a track's General MIDI instrument (1–128) or the drum kit (only an empty track changes between drums and pitched); without a program, a picker |
| `:track-volume 80` (`+10`, `-10`) / `:track-volume-up` / `:track-volume-down` | the track's volume in percent |
| `:track-color #rrggbb` / `:track-color off` / `:track-color` | the track's color (saved in `.muse` files) / back to the theme's / the next palette color |
| `:mute [n\|audio\|master]` / `:solo [n\|audio\|master]` | toggle mute / solo of track n (default: the cursor's), the audio track, or the synth master. A row is soloed, muted or neither: turning one on turns the other off. With any row soloed only soloed rows play; a muted row is silent. The master's mute is on when every synth track is muted (it mutes or unmutes them all); its solo is on when any synth track is soloed (it solos them all, or clears their solos) |

## Properties pane

`<leader>p` shows or hides the properties pane, docked full height along the right of the
window (`ui.properties = true` shows it from the start; workspaces remember it and its tab). It has
three tabs (SCORE, TRACK, LYRICS). Each property is a label with its value in a box across the
pane's width; they come in sections (with chevrons) that fold. In the GUI keys are the status
line's size, values and tabs 1.2 times and section titles 1.5 times; tab titles are uppercase and
section titles small capitals (in the terminal too, as small-capital letters). Color and tuning
have a narrower box with the color's hex code or the strings' pitches beside it in grey.

- **SCORE** — *Info* (title, subtitle, artist, album, words, music, copyright, tab author,
  instructions) and *Page* (page size, margins and scale, as Guitar Pro 5 files keep them; muse
  doesn't print with them yet). Tempo and the pickup bar are edited in the score itself.
- **TRACK** — the track under the score cursor (`n`/`p` change it): *Info* (name, kind, color),
  *Notation* (tuning, frets, capo, and the key *Staves* over one field per staff, top to bottom, with an empty "Add Staff" field at the bottom; each opens a picker of staff kinds, shown with an icon, and a field's handle drags it to another place, a line showing where it lands) and *Sound*
  (instrument, MIDI channel, bank, volume, pan, mute, solo).
- **LYRICS** — the cursor track's lyrics as one text area, in Guitar Pro lyric syntax: syllables
  split by spaces and hyphens, laid onto the notes from the bar they started at (the cursor's
  bar for new ones). As in Guitar Pro, line breaks are spaces.

Every edit is an ordinary, undoable score edit. `<leader>P` focuses the pane (again: back to the
score pane). It stays Normal mode; while the pane has the focus its keys are
`[keymap.properties]`, then the usual ones:

| keys | command |
| --- | --- |
| `j` `<Down>` `<Tab>` / `k` `<Up>` `<S-Tab>` | select the next / previous property |
| `L` `<C-Tab>` / `H` `<C-S-Tab>`, `1` `2` `3` | next / previous tab, the score / track / lyrics tab |
| `<CR>` `i` `a` | edit the property (insert mode). The score updates as you type, and the edit is one undo step. `<CR>` keeps a single line, `<Esc>` and `<C-c>` cancel it; `<Left>` `<Right>` `<Home>` `<End>` move, `<C-u>` clears. The lyrics text area is edited like vim: `<Esc>` goes to normal mode (`h` `j` `k` `l` `w` `b` `e` `0` `^` `$` `gg` `G` move; `x` `D` `dd` delete; `i` `a` `I` `A` `o` `O` `<CR>` insert; `u` `<C-r>` undo and redo), each change and each insert session an undo step; `<Esc>` in normal mode leaves it. The status line shows the current mode. Color opens a picker of named colors, each with a swatch and its hex code; type `#rrggbb` or `rgb(r,g,b)` for your own. Tuning opens a picker of common tunings of the track's instrument (Drop D, DADGAD, Open C …), searchable by name or description; type the notes (`D2 A2 D3 G3 B3 E4`) for your own. Instrument opens its picker; mute and solo are switches (a click flips them, with on/off beside them); a section header folds |
| `h` `l` / `<Left>` `<Right>` | a switch off / on; a choice (color, tuning, instrument, staff kind) the previous / next one, without wrapping; a number down / up by one |
| `c` / `d` | clear the property's text and edit it / just clear it |
| `<A-j>` `<A-k>` | swap the selected staff with the one below / above |
| `za` `zc` `zo` | toggle / close / open the section |
| `x` | same as `d`: clear the text, lyrics or the track's own color, or remove the selected staff |
| `/` | search: a field opens at the bottom of the pane and jumps to the first key or value with what is typed (any case, unless a capital is typed); `<CR>` keeps the jump and sets the `/` register, `<Esc>` goes back |
| `n` `N` | jump to the next / previous match of the `/` register (in the lyrics text area, normal mode: through the text, `/` too) |
| `<Esc>` `q` | back to the score pane |
| `<leader><leader>` `<leader>s` `<leader>f` `:` | play/pause, stop, follow, command line |

Clicking a tab shows it, a row selects it and a double click edits it; a click on a switch flips it. Instruments and staves are shown with an icon (a guitar for every kind of guitar, and so on). In the lyrics text area being edited the mouse works as in any text field: a click places the caret, a drag selects (past the top or bottom it scrolls), a double click in insert mode selects the word (in normal mode it enters insert mode at the click), typing replaces the selection, `<BS>`, `<Del>` and in normal mode `d` `x` remove it and `c` changes it. `<C-a>` selects all; `<C-c>` copies, `<C-x>` cuts and `<C-v>` pastes through the system clipboard (`wl-copy`/`wl-paste`, `xclip`, `xsel`, `pbcopy`/`pbpaste` or PowerShell, whichever is there); in normal mode `y` copies a selection, `yy` the line, and `p` / `P` paste after / before the caret. The wheel scrolls the pane and moves the selection in pickers. In the lyrics text area `j` and `k` move by the lines as wrapped on screen.

| command | |
| --- | --- |
| `:properties [on\|off]` / `:properties-focus` | show or hide the pane / focus it |
| `:properties-tab [score\|track\|lyrics]` | show a tab (none: the next) |
| `:properties-fold [open\|close]` | open, close or (no argument) toggle the selected section |

## Editing palette

`<leader>e` shows or hides the editing palette, docked full height along the left of the window
(`ui.palette = true` shows it from the start). Like Guitar Pro's editing panel it is a grid of
small icon buttons, wrapped to the pane's width, in collapsible sections: durations (whole to
64th, dotted, triplet, rest), pitch (sharp, flat, natural, octave, tie, dead and ghost notes),
dynamics (`ppp` to `fff`), articulations (accent, staccato, let ring, palm mute) and effects
(hammer-on/pull-off, slide, bend, vibrato, harmonic, tremolo picking, tap/slap/pop, strokes). In
the GUI the mouse resting on a button shows its name; the terminal draws short labels and shows
the selected button's name on the pane's bottom row. Each button runs an ordinary command
(`:duration 8`, `:toggle palm-mute`, `:dynamic mf` …) on the score pane worked on. The buttons in
force at the cursor (the beat's duration, its dynamic and techniques) are tinted with the accent
and follow the cursor as it moves; the mouse-over and the keyboard-selected button are a neutral
gray. `<leader>E` focuses the pane (again: back to the score); while it has the focus its keys
are `[keymap.palette]`, then the usual ones:

| keys | command |
| --- | --- |
| `h` `l` `<Left>` `<Right>` | select the button to the left / right in its line (no wrapping) |
| `j` `<Down>` `<Tab>` / `k` `<Up>` `<S-Tab>` | select the button below / above (keeping the column), through the section titles |
| `<CR>` `<Space>` | apply the selected button (on a section title: fold or unfold it) |
| `za` `zc` `zo` | toggle / close / open the section |
| `<Esc>` `q` | back to the score pane |

A click applies a button (and returns to the score pane) or folds a section; the wheel scrolls.

| command | |
| --- | --- |
| `:palette [on\|off]` / `:palette-focus` | show or hide the pane / focus it |
| `:palette-down` `:palette-up` `:palette-left` `:palette-right` `:palette-activate` `:palette-fold [open\|close]` | move, apply, fold |
| `:dynamic ppp\|pp\|p\|mp\|mf\|f\|ff\|fff` | set the dynamic of the note under the cursor (velocity 15 … 127) |

## Playing

Playing is not a mode: Normal, Visual, Insert and the command line all keep working while a
song plays, and edits are heard at once. One file plays at a time; its tab in the bufferline
shows ▶ (⏸ when paused) in place of its file-type icon.

| keys                                                                    | command                                                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<leader><leader>`                                                      | play from the edit cursor / pause. Resuming carries on from where it paused if the cursor is still there; otherwise play starts at the cursor. In another file, it stops the playing file and plays this one                                            |
| `<leader>s`                                                             | stop: the playhead goes back to where playback started (and the cursor too, when following)                                                                                                                                                             |
| `<leader>f`                                                             | follow playback. **On**: the edit cursor follows the red playhead (hidden meanwhile, so only the red bar shows; `ui.hide_cursor_while_following = false` shows it gliding along), and moving the cursor by hand (`}`, `{`, `hjkl`, marks…) jumps the playhead to it. **Off**: the playhead plays on by itself while the cursor moves and edits independently |
| `<leader>l` `<leader>m` `<leader>-` `<leader>=` `<leader>a` `<leader>t` | loop, metronome, speed, mix, sync tap                                                                                                                                                                                                                   |

## Insert mode (M4, done)

Insert-mode keys depend on where the cursor is: on a **tab** (or drum lanes), on a **standard**
staff, on a **slash** staff or on **chant**. Each has its own keymap, looked up first
(`[keymap.insert-tab]`, `[keymap.insert-standard]`, `[keymap.insert-slash]`,
`[keymap.insert-chant]` in `config.toml`), then the keys shared by all (`[keymap.insert]`).
The status line shows where the cursor is (`string 3`, `G4`, `slash`, `chant h`).

Shared keys:

| keys                         | command                                                         |
| ---------------------------- | --------------------------------------------------------------- |
| `<Space>` / `<Right>`        | next beat, appending one (or moving to the next bar) at the end |
| `<Left>` / `<Up>` / `<Down>` | previous beat / row up / row down (across staves, as `k` / `j`) |
| `<CR>`                       | new beat after the cursor                                       |
| `<BS>` / `<Del>`             | delete the note                                                 |
| `r` / `.` / `<C-t>`          | rest / dot / triplet                                            |
| `+` / `-` / `<A-1>`…`<A-7>`  | longer / shorter / whole … 64th                                 |
| `<C-a>` / `<C-x>`            | the cursor note a semitone up / down                            |
| `<Esc>`                      | back to Normal (the whole session is one undo step)             |

On a **tab**:

| keys                                        | command                                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `0`–`9`                                     | fret on the cursor string (two quick digits = two-digit fret)                              |
| `h` `b` `s` `~` `x` `m` `l` `g` `a` `n` `t` | hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |

On drum tracks _(M7)_, letters add or remove an instrument on the cursor beat: `k` bass drum,
`s` snare, `h` closed hi-hat, `o` open hi-hat, `p` pedal hi-hat, `c` crash, `d` ride, `t` high
tom, `m` mid tom, `f` floor tom (`r` is still a rest, `g`/`a` ghost and accent).

On a **standard** staff the cursor sits on a line or space (`<Up>`/`<Down>` move it a step; past
the staff they go on to the tab below or the next staff):

| keys                                  | command                                                                                                                                                   |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a`–`g`                               | that note, in the key, nearest the cursor (`:note-letter`): it replaces the note the cursor is on, or joins the chord; the cursor moves onto it. Fretted tracks pick a string |
| `#` / `&` / `=`                       | the cursor note sharp / flat / natural (`:accidental`)                                                                                                    |
| `t` `h` `s` `~` `>` `(` `x`           | tie, hammer/pull, slide, vibrato, accent, ghost, dead                                                                                                     |

On a **slash** staff: `/` puts a slash on the beat (`:slash`); `t` tie, `>` accent.

On **chant**, the cursor is on a note. On a chant track (opened from GABC) these edit the chant
itself; on any other track they edit the track's notes (shown on its other staves too) and keep
the chant signs with them, the syllables coming from its lyrics:

| keys                                      | command                                                                         |
| ----------------------------------------- | ------------------------------------------------------------------------------- |
| `a`–`m` / `A`–`M`                         | the cursor note's GABC pitch / as a punctum inclinatum (`:chant-note`)           |
| `<Space>` / `/`                           | a new note after the cursor note, in the same neume / starting a new neume      |
| `<CR>`                                    | a new syllable after the cursor syllable                                        |
| `v` `s` `o` `q` `w` `~`                   | virga, stropha, oriscus, quadratum, quilisma, liquescent (toggles, `:chant-mod`) |
| `.` `_` `'` `-`                           | mora, episema, ictus, initio debilis                                            |
| `,` `;` `:`                               | a division after the syllable (`:` again makes `::`; `:chant-bar`)              |
| `<BS>` / `<Del>`                          | delete the note                                                                 |
| `<Right>`                                 | next note                                                                       |

## Command mode (selected)

`:` opens the **command palette**, a box near the top of the window (in the terminal UI too).
As you type, the matches for the word being typed are listed below it: command names with a short
description on the right, then each command's arguments — file names for `:open`, `:w`,
`:view-new` and `:audio` (folders first, only files muse can open or write) and the values of
`:layout`, `:mix`, `:volume`, `:track-add`, `:tuning`, `:toggle`, `:duration`, `:clef`,
`:interpretation`. `<Tab>` fills in a unique match (or the common part), and `<Tab>`/`<S-Tab>`
or `<Down>`/`<Up>` move through the list, putting the selected match on the line. `<Enter>`
runs the line, `<Esc>` closes the palette, `<C-p>`/`<C-n>` recall earlier commands.

Matching is **fuzzy** by default: the typed letters may appear anywhere in a candidate, in order
(`:sd` finds `sync-detect`, `speed-down`), with prefix matches first, then matches at word
starts and in runs; matched letters are highlighted. One candidate starting with what you typed
still completes outright on `<Tab>`. Matching also ignores letter case. Turn either off with
`ui.completion_fuzzy = false` / `ui.completion_ignore_case = false` in `config.toml`, or change
them on the fly with `:completion-fuzzy [on|off]` and `:completion-ignore-case [on|off]`
(without an argument they toggle); bind them under `[keymap.command]` to flip them while typing,
e.g. `"<C-g>" = "completion-ignore-case"`, `"<C-f>" = "completion-fuzzy"`.

| command                                                              |                                                                                                                                                                                             |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `:open {file}` / `:w [file]` / `:q` (quit) / `:wq` (write all, quit) / `:q!`                  | _(M2)_ `:w file.ext` exports by extension                                                                                                                                                   |
| `:wa` (`:wall`) / `:pane-close` | write every changed file / close the focused pane (its view with the last pane) |
| `:split` / `:vsplit`                                                 | _(M1)_                                                                                                                                                                                      |
| `:track {n}`                                                         | _(M1)_                                                                                                                                                                                      |
| `:bar {n}` / `:{n}`                                                  | _(M1)_                                                                                                                                                                                      |
| `:set {option}[=value]`                                              |                                                                                                                                                                                             |
| `:source`                                                            | reload config _(M1)_                                                                                                                                                                        |
| `:time 3/4` / `:key -2` / `:tempo 96`                                | time / key signature from the cursor bar, tempo at the cursor bar                                                                                                                           |
| `:repeat-start` / `:repeat-end [n]` / `:section NAME` / `:text TEXT` | bar marks and beat text                                                                                                                                                                     |
| `:note C#4` / `:voice 2`                                             | set the cursor note's pitch / edit another voice                                                                                                                                            |
| `:tuning drop-d` or `:tuning D2 A2 D3 G3 B3 E4` / `:capo 2`          | retune keeping pitches                                                                                                                                                                      |
| `:refinger` / `:transpose 2`                                         | re-finger the track to minimise hand movement / transpose                                                                                                                                   |
| `:track-add bass` / `:track-delete` / `:new`                         | tracks and new scores (`:track-add piano` gets a grand staff)                                                                                                                                 |
| `:staff-add [staff]`                                                | add a staff at the bottom of the cursor's track; staves only show the track's notes, so each track can have as many as you like |
| `:staff {staff}` | show the cursor's staff another way: `treble`, `bass` or `grand [split]` (a grand staff splits at middle C, C4, unless a note such as `G3` is given), `tab`, `tab+` (tab with rhythm stems), `slash`, `chant`. A tab on a track without strings puts its notes on strings (refused if one would not fit) |
| `:staff-remove [n]` / `:staff-move {n\|+N\|-N}`                      | remove a staff (1 = top; the cursor's by default; the last one stays) / move the cursor's staff to position n, or down / up |
| `:lyrics [verse] TEXT` _(M7)_                                        | syllables from the cursor on, one per note, in Guitar Pro syntax: spaces and hyphens separate syllables (`he-si-tate`), an extra space skips a note, `+` joins words on one note; verse 1–5 |
| `:lyrics-clear [verse]` _(M7)_                                       | remove a verse (all verses without a number) from the track                                                                                                                                 |
| `:speed 75`                                                          | playback speed in percent                                                                                                                                                                   |

### Workspaces

A workspace saves the views — each with its pane splits, name and bufferline of loaded files —
and each pane's file, cursor, recent files, layout and waveform strip, each pane's zoom, follow, chant
view, the window waveform bar, marks (of saved files) and recorded macros, to
`~/.local/share/muse/workspaces/NAME.toml` (or a path). A workspace is always active, and is
saved again whenever the session changes and when muse quits. Until you choose one, it is
`default`. `muse` reopens the workspace used last; `muse --workspace NAME` starts with another
(created from scratch if it doesn't exist yet). `muse FILE…` opens just those files, without a
workspace, so one-off edits leave your workspaces alone — `:workspace-save NAME` keeps such a
session if you want it (with `--workspace` too, the files open in that workspace).

| command                  |                                                                                          |
| ------------------------ | ---------------------------------------------------------------------------------------- |
| `:workspace-save [name]` | save the session as a workspace and make it active (no name: the active one)             |
| `:workspace-open name`   | replace the session with a saved workspace (`:workspace-open!` discards unsaved changes) |
| `:workspace-close`       | save and leave the active workspace for `default`                                        |

### Chant _(M6)_

On a chant track the cursor moves note by note; `<C-a>`/`<C-x>` move a note up or down (adding a
flat or natural when needed), `x` deletes it and `u` undoes, as on any track.

| command                                      |                                                                                                       |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `:gabc` / `:gabc fgh` / `:gabc Glo(fg)ri(h)` | show the cursor syllable as GABC; replace its notes, or the syllable itself                           |
| `:interpretation [equalist\|solesmes]`       | rhythm used for playback and the modern transcription                                                 |
| `:clef c3`                                   | change the chant clef (`c1`–`c4`, `f1`–`f4`, `cb3` for a flat clef); notes keep their staff positions |
| `:chant-view`                                | square notation / modern transcription of chant tracks                                                |
| `:modern-to-chant`                           | open the cursor track's melody as chant in a new buffer (syllables from lyrics)                       |
| `:chant-to-modern`                           | turn the chant track into an ordinary staff track                                                     |

### Play-along _(M5)_

| command                                    |                                                                                                                                                                                                          |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `:audio [file]` / `:audio off`             | attach a recording (mp3, m4a/aac, flac, ogg, wav; relative to the score). Without a file, the best name match next to the score or in its `mp3/`/`audio/` folder                                         |
| `:sync`                                    | start (or stop) sync tapping from the cursor bar: play, then `<leader>t` on each downbeat                                                                                                                |
| `:sync-tap` / `:sync-clear`                | add a sync point at the playing bar / remove all sync points                                                                                                                                             |
| `:sync-detect`                             | beat-detection assist: fill in sync points from the last existing one (tap the first downbeat first for best results)                                                                                    |
| `:mix [both\|audio\|midi]`                 | what plays when a recording is attached (no argument cycles)                                                                                                                                             |
| `:volume midi\|audio\|metronome\|master {percent}` | source levels (`master` scales the whole output)                                                                                                                                                                                            |
| `:mute` / `:solo [n\|audio\|master]`       | toggle a track's (cursor track by default), the audio track's or the master's mute / solo                                                                                                                                                |
| `:loop [a b]` / `:loop-toggle`             | loop bars a–b (1-based), or the selection / cursor bar                                                                                                                                                   |
| `:trainer start target step`               | speed trainer: start at `start`%, add `step`% each loop up to `target`% (e.g. `:trainer 60 100 5`)                                                                                                       |
| `:pitch {semitones}`                       | shift the recording's pitch (−12…12, keeps tempo)                                                                                                                                                        |
| `:metronome` / `:count-in` / `:waveform-strip` | toggles (`:waveform-strip`: the focused pane's strip) |
| `:waveform [on\|off]`                      | the waveform tool pane: the score pane's audio waveform, marking the play position (or the cursor when stopped); no argument toggles |
| `:global-tool TOOL`                        | switch a tool (tracks, waveform, fretboard, piano, tuner) between global (along the window bottom) and local (under its score pane) |
| `:scrollbar [on\|off]`                    | the scrollbars on every score pane: down the right side and along the bottom, each shown when the score scrolls that way (a bar wider than the pane, a zoomed page, a tall horizontal layout), in their own gutter (the score never draws under it). Click or drag it to scroll (the view then stays put until the cursor moves). `ui.scrollbar` sets the default (on) |
| `:progress-bar [on\|off]`                  | the progress bar along the bottom of every score pane: the sections (letter and title), a translucent red mark at the play position (or the cursor), and the time; clicking it moves the cursor to that bar. `ui.progress_bar` sets the default (on) |
| `:export {file}`                           | export by extension                                                                                                                                                                                      |
| `:layout [page\|vertical\|horizontal]`     | GUI score layout of the focused pane _(M3)_                                                                                                                                                              |
