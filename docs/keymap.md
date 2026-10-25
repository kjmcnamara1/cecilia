# Default keymap

All bindings can be overridden in `config.toml` under `[keymap.<mode>]`. `<leader>` defaults to `<Space>`.
Items marked _(M1)_ are implemented in the first milestone; the rest are the planned defaults.

## Normal mode

### Cursor movement

| keys                | command                                         |        |
| ------------------- | ----------------------------------------------- | ------ |
| `h` / `l`           | previous / next beat                            | _(M1)_ |
| `j` / `k`           | next / previous string (lower / higher)         | _(M1)_ |
| `w` / `b`           | next / previous beat with notes                 | _(M1)_ |
| `}` / `{`           | next / previous bar                             | _(M1)_ |
| `0` / `$`           | first / last beat of bar                        | _(M1)_ |
| `gg` / `G`          | first bar / last bar (`[count]G` = bar _count_) | _(M1)_ |
| `J` / `K`           | next / previous track                           | _(M1)_ |
| `]s` / `[s`         | next / previous section marker                  |        |
| `m{a-z}` / `'{a-z}` | set / jump to mark                              |        |
| `<C-d>` / `<C-u>`   | half page down / up                             | _(M1)_ |

### Playback

| keys                      | command                   |        |
| ------------------------- | ------------------------- | ------ |
| `<leader><leader>`        | play / pause from cursor  | _(M1)_ |
| `<leader>s`               | stop                      | _(M1)_ |
| `<leader>f`               | toggle following playback. A red bar always marks the playhead; while following, the editing cursor and the view move with it. Turned off (even mid-playback), the cursor stays where it is and is moved only by `hjkl` | _(M1)_ |
| `<leader>l`               | loop the selected bars (or the cursor bar) / loop off | _(M5)_ |
| `<leader>c`               | toggle count-in           | _(M5)_ |
| `<leader>m`               | toggle metronome          | _(M5)_ |
| `<leader>-` / `<leader>=` | speed −5% / +5%           | _(M5)_ |
| `<leader>a`               | cycle mix: audio + MIDI / audio only / MIDI only | _(M5)_ |
| `<leader>t`               | sync tap: place the next bar at the current audio time | _(M5)_ |
| `<leader>w`               | show / hide the waveform strip of the focused pane (GUI) | _(M5)_ |
| `<leader>v`               | cycle the focused pane's score layout (vertical / page / horizontal); new splits and views copy it | _(M3)_ |
| `<C-=>` / `<C-->` / `<C-0>` | zoom in / out / reset (GUI; also Ctrl+wheel) | _(M3)_ |

### Buffers, panes, views

The model, as in Neovim with bufferline.nvim: a **pane** shows one **buffer** (an open file);
a **view** is a layout of panes with its own set of loaded buffers, shown as tabs on the left of
the tabline; the views are listed on the right of the tabline; a **workspace** saves the views.
A file open in several views is one shared buffer (edits and undo show in all of them).

Each buffer tab shows a file-type icon, the name (shortened in the middle when longer than
`tabline.truncation.max_width`, keeping the first `prefix_width` and last `suffix_width`
characters), and on mouse hover an `×` to close it (`●` marks unsaved changes). Click a tab to
show it in the focused pane, click `×` or middle-click to close it, click a view to switch.

Closing a buffer's tab removes it from the view; panes showing it go back to the most recent
other buffer they showed (the view's only pane falls back to the neighbouring tab) and close if
there is none. The buffer is unloaded once no view has it; unsaved changes then stop the close
unless forced (`:bdelete!`).

| keys                | command                         |        |
| ------------------- | ------------------------------- | ------ |
| `<leader>b`         | buffer picker                   |        |
| `<leader>V`         | view picker                     |        |
| `<leader>W`         | workspace picker                |        |
| `L` / `H`           | next / previous buffer tab      |        |
| `<leader>x`         | close the buffer's tab          |        |
| `gt` / `gT`         | next / previous view            |        |
| `<C-w>s` / `<C-w>v` | split horizontally / vertically | _(M1)_ |
| `<C-w>h/j/k/l`      | focus pane left/down/up/right   | _(M1)_ |
| `<C-w>q`            | close pane (the last one closes the view) | _(M1)_ |

**Pickers** open in the command palette's box and list the view's buffers (with their folder,
● when modified), the views (with their files) or the saved workspaces (with their view and file
counts). Typing filters the list with the same matching as completion (fuzzy, case-insensitive
by default) and highlights what matched; the top match is selected — `<Up>`/`<Down>` (or
`<Tab>`/`<S-Tab>`, `<C-p>`/`<C-n>`) move, `<Enter>` switches, `<Esc>` cancels. The buffer and
view pickers also work while a song plays.

| command | |
| --- | --- |
| `:open {file}` | open a file in the focused pane, adding it to the view's tabs |
| `:pick-buffer` / `:pick-view` / `:pick-workspace` (`:pick buffer\|view\|workspace`) | open a picker |
| `:bnext` / `:bprev` / `:buffer N` | next / previous / Nth buffer tab |
| `:bdelete [N]` / `:bdelete! [N]` | close the focused (or Nth) buffer tab / discarding unsaved changes |
| `:view-new [file]` | new view: a copy of the current view's tabs, just the current buffer, or empty (`tabline.new_view`) |
| `:view-close` / `:view-close!` | close the view (buffers only it had are unloaded) |
| `:view N` / `:view-next` / `:view-prev` | switch views |
| `:view-rename [name]` | name the view in the tabline (no name: its number) |

### Editing (M4, done)

| keys | command |
| --- | --- |
| `i` / `a` | insert at the cursor beat / append a beat and insert |
| `x` | delete the note on the cursor string (`[count]x`) |
| `d{motion}` / `dd` | delete beats / bars (`[count]dd`) |
| `y{motion}` / `yy`, `p` / `P` | yank beats or bars / put after / before |
| `c{motion}` / `cc` | change (delete, then insert) |
| `"{reg}` | use register for the next yank/delete/put |
| `.` | repeat the last change (including an insert session) |
| `u` / `U` / `g-` / `g+` | undo / redo / older / newer state across undo branches |
| `q{reg}` … `q` / `[count]@{reg}` / `@@` | record / play / replay a macro |
| `v` / `V` | visual beats / visual bars (then `d`, `y`, `c`, `x`) |
| `o` / `O` | new bar after / before |
| `+` / `-` | longer / shorter note value |
| `<C-a>` / `<C-x>` | pitch up / down a semitone (moves string when the fret runs out) |
| `r` | make the beat a rest |
| `th` `tb` `ts` `tv` `tx` `tm` `tl` `tg` `ta` `tn` `tt` | toggle hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| `t.` / `t3` | toggle dot / triplet |
| `ZZ` | write and close |

### Other

| keys        | command          |        |
| ----------- | ---------------- | ------ |
| `:`         | command line     | _(M1)_ |
| `<leader>?` | which-key / help |        |
| `ZZ`        | write and quit   |        |

## Play mode

Starting playback (`<leader><leader>`) enters **Play** mode (status `PLAY`, `PAUSED` while
paused). It has its own keymap, `[keymap.play]`: by default the Normal-mode bindings of
playback, navigation, pane/tab, view and `:` commands, but no editing keys, so stray keys can't
change the score while it plays.

| keys | command |
| --- | --- |
| `<leader><leader>` | pause / resume (resume carries on from where it paused) |
| `<Esc>` / `<leader>s` | stop: back to where playback started, and to Normal mode |
| `h j k l w b { }` … | move the cursor (with following on, it is moved along with the playhead) |
| `<leader>f` `<leader>l` `<leader>m` `<leader>-` `<leader>=` `<leader>a` `<leader>t` | follow, loop, metronome, speed, mix, sync tap |
| `:` | command line; returns to Play mode |

The mode ends by itself when the song finishes.

## Insert mode (M4, done)

| keys | command |
| --- | --- |
| `0`–`9` | fret on the cursor string (two quick digits = two-digit fret; MIDI key on drum/pitched tracks) |
| `<Space>` / `<Right>` | next beat, appending one (or moving to the next bar) at the end |
| `<Left>` / `<Up>` / `<Down>` | previous beat / string up / string down |
| `<CR>` | new beat after the cursor |
| `<BS>` / `<Del>` | delete the note |
| `h` `b` `s` `~` `x` `m` `l` `g` `a` `n` `t` | hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| `r` / `.` / `<C-t>` | rest / dot / triplet |
| `+` / `-` / `<A-1>`…`<A-7>` | longer / shorter / whole … 64th |
| `<Esc>` | back to Normal (the whole session is one undo step) |

On drum tracks _(M7)_, letters add or remove an instrument on the cursor beat: `k` bass drum,
`s` snare, `h` closed hi-hat, `o` open hi-hat, `p` pedal hi-hat, `c` crash, `d` ride, `t` high
tom, `m` mid tom, `f` floor tom (`r` is still a rest, `g`/`a` ghost and accent).

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

| command                                          |                                |
| ------------------------------------------------ | ------------------------------ |
| `:open {file}` / `:w [file]` / `:q` / `:wq` / `:q!` | _(M2)_ `:w file.ext` exports by extension |
| `:split` / `:vsplit`                             | _(M1)_                         |
| `:track {n}`                                     | _(M1)_                         |
| `:bar {n}` / `:{n}`                              | _(M1)_                         |
| `:set {option}[=value]`                          |                                |
| `:source`                                        | reload config _(M1)_           |
| `:time 3/4` / `:key -2` / `:tempo 96` | time / key signature from the cursor bar, tempo at the cursor bar |
| `:repeat-start` / `:repeat-end [n]` / `:section NAME` / `:text TEXT` | bar marks and beat text |
| `:note C#4` / `:voice 2` | set the cursor note's pitch / edit another voice |
| `:tuning drop-d` or `:tuning D2 A2 D3 G3 B3 E4` / `:capo 2` | retune keeping pitches |
| `:refinger` / `:transpose 2` | re-finger the track to minimise hand movement / transpose |
| `:track-add bass` / `:track-delete` / `:new` | tracks and new scores (`:track-add piano` adds a grand staff: two staves of one part) |
| `:lyrics [verse] TEXT` _(M7)_ | syllables from the cursor on, one per note, in Guitar Pro syntax: spaces and hyphens separate syllables (`he-si-tate`), an extra space skips a note, `+` joins words on one note; verse 1–5 |
| `:lyrics-clear [verse]` _(M7)_ | remove a verse (all verses without a number) from the track |
| `:speed 75` | playback speed in percent |

### Workspaces

A workspace saves the views — each with its pane splits, name and bufferline of loaded files —
and each pane's file, cursor, recent files, layout and waveform strip, plus zoom, follow, chant
view and the window waveform bar, to
`~/.local/share/muse/workspaces/NAME.toml` (or a path). While one is active it is saved again
whenever the session changes and when muse quits. Start with one using `muse --workspace NAME`.

| command | |
| --- | --- |
| `:workspace-save [name]` | save the session as a workspace and make it active (no name: the active one) |
| `:workspace-open name` | replace the session with a saved workspace (`:workspace-open!` discards unsaved changes) |
| `:workspace-close` | stop saving the session to the active workspace |

### Chant _(M6)_

On a chant track the cursor moves note by note; `<C-a>`/`<C-x>` move a note up or down (adding a
flat or natural when needed), `x` deletes it and `u` undoes, as on any track.

| command | |
| --- | --- |
| `:gabc` / `:gabc fgh` / `:gabc Glo(fg)ri(h)` | show the cursor syllable as GABC; replace its notes, or the syllable itself |
| `:interpretation [equalist\|solesmes]` | rhythm used for playback and the modern transcription |
| `:clef c3` | change the chant clef (`c1`–`c4`, `f1`–`f4`, `cb3` for a flat clef); notes keep their staff positions |
| `:chant-view` | square notation / modern transcription of chant tracks |
| `:modern-to-chant` | open the cursor track's melody as chant in a new buffer (syllables from lyrics) |
| `:chant-to-modern` | turn the chant track into an ordinary staff track |

### Play-along _(M5)_

| command | |
| --- | --- |
| `:audio [file]` / `:audio off` | attach a recording (mp3, m4a/aac, flac, ogg, wav; relative to the score). Without a file, the best name match next to the score or in its `mp3/`/`audio/` folder |
| `:sync` | start (or stop) sync tapping from the cursor bar: play, then `<leader>t` on each downbeat |
| `:sync-tap` / `:sync-clear` | add a sync point at the playing bar / remove all sync points |
| `:sync-detect` | beat-detection assist: fill in sync points from the last existing one (tap the first downbeat first for best results) |
| `:mix [both\|audio\|midi]` | what plays when a recording is attached (no argument cycles) |
| `:volume midi\|audio\|metronome {percent}` | source levels |
| `:mute [n]` / `:solo [n]` | toggle a track's mute / solo (cursor track by default) |
| `:loop [a b]` / `:loop-toggle` | loop bars a–b (1-based), or the selection / cursor bar |
| `:trainer start target step` | speed trainer: start at `start`%, add `step`% each loop up to `target`% (e.g. `:trainer 60 100 5`) |
| `:pitch {semitones}` | shift the recording's pitch (−12…12, keeps tempo) |
| `:metronome` / `:count-in` / `:waveform` | toggles (`:waveform` only in the focused pane) |
| `:waveform-global [on\|off]` | a waveform bar across the window above the status line, for the focused pane's file, marking the play position (or the cursor when stopped); no argument toggles (`ui.global_waveform` sets the default) |
| `:export {file}`                                 | export by extension            |
| `:layout [page\|vertical\|horizontal]`            | GUI score layout of the focused pane _(M3)_ |
