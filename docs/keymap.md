# Default keymap

All bindings can be overridden in `config.toml` under `[keymap.<mode>]`. `<leader>` defaults to `<Space>`.
Items marked _(M1)_ are implemented in the first milestone; the rest are the planned defaults.

## Key notation

Keys are written as in Vim: a character is itself (`gg`, `Q`), anything else or with modifiers
goes in angle brackets (`<Space>`, `<CR>`, `<Esc>`, `<Tab>`, `<F5>`, `<Up>`, `<lt>`, `<leader>`).
Modifier prefixes are case-insensitive and combine: `<C-A-S-q>`.

| Prefix | Modifier | Aliases |
| --- | --- | --- |
| `C-` | Ctrl | `Ctrl-` |
| `A-` | Alt | `M-`, `Alt-`, `Meta-` |
| `S-` | Shift | `Shift-` |
| `D-` | Super (Windows / Cmd) | `Super-`, `Win-`, `Cmd-` |

Keys are shown (which-key, `pick-keymap`) in the order `C-A-S-D-`.

Shift rule: Shift on a printable character without Ctrl, Alt or Super is part of the character,
so `Q` and `<S-q>` are the same key and typing is unchanged. Shift is a separate modifier

- with Ctrl, Alt or Super: `<C-S-q>` is a different key from `<C-q>` (a binding never matches the
  other), and `<A-S-j>` is `<A-J>`;
- on keys that are not characters: `<S-F5>`, `<S-Up>`, `<S-Home>`, `<C-S-Tab>`. `<S-Tab>` is
  Shift+Tab (`BackTab`). Shift on `<Esc>`, `<CR>` and `<BS>` alone is ignored.

`<C-W>` is `<C-w>` (Ctrl letters are lower case); write `<C-S-w>` for Ctrl+Shift+W.

The GUI reports every modifier. In the terminal, Ctrl+Shift+letter, Super and Shift on
non-character keys need the kitty keyboard protocol (kitty, WezTerm, foot, Ghostty, Alacritty
and others); cecilia enables it when the terminal supports it and restores the terminal on exit
and on panic. On other terminals Ctrl+Shift+Q sends the same bytes as Ctrl+Q, so `<C-S-q>` can
only be used in the GUI there, and `<D-…>` bindings never fire.

## Normal mode

### Cursor movement

| keys                      | command                                                                                                                                                  |        |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `h` / `l`                 | previous / next beat, staying on the same string, line or space                                                                                          | _(M1)_ |
| `j` / `k`                 | next / previous row on the cursor's staff: tab strings, or staff lines and spaces, with four ledger-line positions beyond the staff's edges to place extra notes. With `ui.jk_moves_between_staves = true` they go on into the next / previous staff instead (`:string-down`, `:string-up`) | _(M1)_ |
| `<A-j>` / `<A-k>`         | cursor to the next / previous staff of the part (`:staff-next`, `:staff-prev`) | |
| `w`                       | next note on the cursor's string (tab) or line / space (standard) that does not continue a tie. On a slash staff or a chant track, the next beat with a note (`:note-row-next`) | |
| `e`                       | end of the tie the cursor note is in (its last note); on a note that is not tied on, or off a note, the end of the next note's tie on the row (`:note-row-end`) | |
| `b`                       | start of the tie the cursor note is in; on the first note of a tie, or off a note, the start of the previous note's tie on the row (`:note-row-prev`)    | |
| `W` / `B`                 | next / previous beat with a note on any string or line (the old `w` / `b`; `:note-next`, `:note-prev`)                                                    | _(M1)_ |
| `}` / `{`                 | next / previous bar                                                                                                                                      | _(M1)_ |
| `^` / `$`, `gh` / `gl`    | first / last beat of the bar that has a note, on any string or line; a bar without notes: its first / last beat (`:bar-first-note`, `:bar-last-note`)   | |
| `0` `<Home>` / `<End>`    | first / last beat of the bar, note or not (`:bar-start`, `:bar-end`)                                                                                      | _(M1)_ |
| `gg` / `G`                | first bar / last bar (`[count]G` = bar _count_)                                                                                                          | _(M1)_ |
| `J` / `K`                 | next / previous track                                                                                                                                    | _(M1)_ |
| `]v` / `[v`, `<A-v>` / `<A-V>` (also insert mode) | edit the next / previous voice of the bar: voices go round through those with notes and one more, which is created empty (`:voice-next`, `:voice-prev`) | |
| `<A-F1>`…`<A-F4>` (also insert mode) | edit voice 1…4 of the bar (`:voice N`); a voice the bar doesn't have yet is created empty. The cursor stays at the same time position, on the beat of that voice sounding there | |
| `]s` / `[s`         | next / previous section (`[count]` sections away; `''` goes back) |        |
| `<leader>S`         | sections picker: every section of the file with its bars; Enter jumps there (`:sections`) | |
| `m{a-z}` / `m{A-Z}`       | set a mark in this file / a global mark (any file)                                                                                                       |        |
| `'{mark}` / `` `{mark} `` | jump to the mark's bar / exact position (a popup lists the marks and where they are); a global mark reopens its file if it was closed. `''` jumps back to before the last jump (also after `gg`, `G`). `` `< `` / `` `> `` (or `'<` `'>`) are the start / end of the last Visual selection, `` `^ `` where Insert mode was last left |        |
| `zz` / `zt` / `zb`        | scroll the view so the cursor is in the middle / at the top / at the bottom of the pane (the left / right in the horizontal layout; the cursor's own staff line when a system is taller than the pane); the cursor does not move, a count is ignored (`:scroll-center`, `:scroll-top`, `:scroll-bottom`) | |
| `<C-d>` / `<C-u>`         | half a screen down / up: half the displayed bars in the horizontal layout, else half the displayed rows of bars (rounded up, at least one; in the page layout half the rows on a page, whatever the zoom) keeping the place in the bar and on the staff | _(M1)_ |

### Voices

A track holds up to four voices (Guitar Pro's): independent rhythmic streams that each fill the bar
on their own, sharing the track's instrument, tuning and mixer settings. The status line shows
`V2` … once the bar has more than one voice or another than the first is being edited (a click
on it goes to the next voice). Beats, durations, ties, `d` `c` `y` `p` and the other edits act on the
voice being edited only; transposing, `:retune` and the like act on all voices. `cc` clears only
the voice being edited (leaving it empty); a new voice is empty too (no rest, nothing filled)
until typed into. The staves draw voices 1 and 3 with stems up and 2 and 4 with stems down,
their rests offset, beats at the same time on the same column and clashing tab frets side by side;
the editing palette starts with a button per voice that switches to it and
highlights the one being edited in that voice's color (`theme.voice_colors`); the terminal tab shows every voice on a shared time grid (the voices not being edited dimmed).
Writing `.gp5` keeps two voices per bar (the status line warns when more with notes are left out);
`.gp`, `.gpx`, `.cc` and MusicXML keep all four.

### Playback

| keys                        | command                                                                                                                                                                                                                 |        |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `<leader><leader>`          | play / pause from cursor                                                                                                                                                                                                | _(M1)_ |
| `<leader>s`                 | stop                                                                                                                                                                                                                    | _(M1)_ |
| `<leader>f`                 | toggle following playback (`:set ui.follow_playback toggle`). A red bar always marks the playhead; while following, the editing cursor and the view move with it. Turned off (even mid-playback), the cursor stays where it is and is moved only by `hjkl` | _(M1)_ |
| `<leader>l`                 | loop the selected bars (or the cursor bar) / loop off                                                                                                                                                                   | _(M5)_ |
| `<leader>c`                 | toggle count-in                                                                                                                                                                                                         | _(M5)_ |
| `<leader>m`                 | toggle metronome                                                                                                                                                                                                        | _(M5)_ |
| `<leader>-` / `<leader>=`   | speed −5% / +5%                                                                                                                                                                                                         | _(M5)_ |
| `<leader>a`                 | cycle mix: audio + MIDI / audio only / MIDI only                                                                                                                                                                        | _(M5)_ |
| `<leader>t`                 | sync tap: place the next bar (chant: the next syllable) at the current audio time                                                                                                                                                                  | _(M5)_ |
| `<leader>w`                 | show / hide the waveform tool pane (`:waveform`) | _(M5)_ |
| `<leader>v`                 | cycle the focused pane's score layout (vertical / page / horizontal); new splits and views copy it                                                                                                                      | _(M3)_ |
| `<leader>d`                 | design mode on / off: adjust the page layout by hand (see _Design mode_ below)                                                                                                                                           | _(M3)_ |
| `<C-=>` / `<C-->` / `<C-0>` | zoom the focused pane in / out / reset (GUI); Ctrl+wheel zooms the pane under the mouse                                                                                                                                                                        | _(M3)_ |

### Design mode

`:design on` (`<leader>d`) lets you fix the page layout by hand, for clashes the automatic layout
cannot see. It works in the page layout only and shows `design` in the status line. Every change
is an undo step (a mouse drag is one step) and is stored in the score: `.ccc` / `.ccm` files keep it,
other formats ignore it, and bars with no adjustment stay automatic.

Keys while design mode is on (`[keymap.design]`, looked up before the normal keys; every other
key keeps its normal meaning, but `+` / `-` stop being the duration keys here). They act on the cursor's line, bar and beat:

| keys | action | command |
| --- | --- | --- |
| `+` / `-` | one more / one fewer bar on the cursor's line (the "+" / "-" buttons) | `:bars-per-line +1` / `-1` |
| `<A-=>` | the cursor's line back to automatic | `:bars-per-line auto` |
| `>` / `<` | cursor bar wider / narrower (10%) | `:bar-width +10` / `-10` |
| `=` | cursor bar back to its automatic width | `:bar-width auto` |
| `<A-l>` / `<A-h>` | cursor beat right / left (0.2 space) | `:beat-nudge +2` / `-2` |
| `<A-0>` | cursor beat back in place | `:beat-nudge reset` |
| `\|` / `_` | new line at the cursor bar / join the cursor bar to the previous line | `:line-break` / `:line-join` |
| `<leader>d` | design mode off | `:design off` |

With the mouse (GUI), handles appear over the page: drag a **bar line** left or right to resize
the two bars beside it (they trade width, the line stays as long as it was); click the **"+" and
"-" buttons** right of a line's end to pull the next bar up onto the line or push its last bar
down (each is one undo step; "-" is dimmed on a one-bar line, "+" on the last line); drag the
**dots under the cursor bar's beats** to nudge them. The page re-flows live while dragging.

In the terminal UI the page layout breaks lines and pages exactly where the GUI's does (the
engraver's lines, scaled to the pane; manual line breaks and bar widths included) and draws each
page in a box (`┌─ Page 1 ─┐`). With design mode on, each line shows `+` / `-` at its right
edge (click: one bar more / fewer), a `↵` in the left margin marks a manual line break, and a
bar line can be dragged sideways (a column is several points of page, so it is coarse; use `>` /
`<` for fine control). Dragging a beat has no terminal equivalent: nudge it with `<A-l>` / `<A-h>`.

Limits: only the page layout is affected (the vertical and horizontal layouts and square-notation
chant tracks ignore the adjustments); lyrics attached from another track do not follow a nudged
beat; adjustments are keyed by bar number, so they follow bars through inserting, deleting and
pasting bars, but a beat nudge stays on its beat number when beats are added or removed.

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
there is none. The buffer is unloaded once no view has it; unsaved changes then ask first: `y` saves
(a score with no file name can't be; `:w <file>` it) and closes, `n` discards, any other key
cancels. `:bdelete!` closes without asking. A new score counts as unchanged until edited.

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
| `<C-w>w` / `<C-w>W` | focus the next / previous pane in layout order, wrapping (`:focus-next`, `:focus-prev`; `<C-w><C-w>` too) | |
| `<C-w>p`            | focus the previously focused pane, back and forth (`:focus-last`; `<C-w><C-p>` too) | |
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
of them by default; `:set ui.global_tools +tuner` / `-tuner` switches one between global and local, moving it if it
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
| `:pick-buffer` / `:pick-view` / `:pick-workspace` (`:pick buffer\|view\|workspace\|recent\|mark\|macro`) | open a picker                                                                                       |
| `:marks` / `:macros` (`:registers`)                                                              | pick a mark to jump to / a macro to run                                                             |
| `:mark x` (`:k x`) / `:delmarks xyz` / `:delmarks!`                                              | set a mark / delete marks / delete this file's lowercase marks                                      |
| `:goto-mark x` / `:run-macro r`                                                                  | jump to a mark / run a macro (for bindings)                                                         |
| `:bnext` / `:bprev` / `:buffer N`                                                                | next / previous / Nth buffer tab                                                                    |
| `:bdelete [N]` / `:bdelete! [N]`                                                                 | close the focused (or Nth) buffer tab / discarding unsaved changes                                  |
| `:view-new [file]`                                                                               | new view: a copy of the current view's tabs, just the current buffer, or empty (`tabline.new_view`); with `ui.keep_files_in_new_view = false`, the dashboard |
| `:view-close` / `:view-close!`                                                                   | close the view (buffers only it had are unloaded); the only view: closes all its buffers and shows the [dashboard](#dashboard)                                                   |
| `:view N` / `:view-next` / `:view-prev`                                                          | switch views                                                                                        |
| `:view-rename [name]`                                                                            | name the view in the tabline (no name: its number)                                                  |

### Editing

Editing is vim-like: the unit of `d` `c` `y` is the beat (motions `h` `l` `w` `e` `b` `^` `$` …)
or, doubled, the bar. `[count]` works on motions and on `o`, `p`, `x`, `dd` …; `.` repeats the
last change, an insert session included. Every edit, and an `i` / `a` / `o` … together with what
is typed after it, is one undo step.

| keys                                                   | command                                                                                           |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `i` / `a`                                              | new rest beat before / after the cursor beat (in the insert duration), then Insert mode on it (`:insert`, `:append`) |
| `I` / `A`                                              | same at the start / end of the bar (`:insert-bar-start`, `:append-bar-end`)                       |
| `o` / `O`                                              | new bar after / before the cursor bar (`[count]` bars), then Insert mode on it (`:bar-open-after`, `:bar-open-before`) |
| `gi`                                                   | Insert mode where Insert mode was last left in this file (on the cursor if never), adding nothing (`:insert-last`) |
| `:reflow`                                              | redraw the barlines of the cursor's voice to fit the time signatures: excess beats move to the start of the next bar, short bars are filled from the next, and a beat straddling a barline is split and tied. Durations and pitches don't change; other voices are left alone. Over every bar, or in Visual mode over the selection only, whose last bar keeps whatever is left over |
| `x`                                                    | delete the one note under the cursor: on the cursor string, or at the cursor line / space (`[count]x`); on a slash staff the beat's slash, i.e. all its notes |
| `d{motion}` / `dd`                                     | delete beats / bars (`[count]dd`). `dl` the beat under the cursor, `dh` the one before it, `dw` `de` `db` `d$` `d^` …  |
| `y{motion}` / `yy`, `p` / `P`                          | yank beats or bars (`yl`, `yh` …, `yy` the bar) / put after / before the cursor                   |
| `c{motion}` / `cc`                                     | change: the beats become rests of the same lengths (the cursor beat for `cl`, the one before it for `ch`, which the cursor moves to) and Insert mode starts on them; `cc` clears the bar of the cursor's track, leaving an empty bar |
| `"{reg}`                                               | use register for the next yank/delete/put                                                         |
| `.`                                                    | repeat the last change (including an insert session), or the last palette tool: applied again at the cursor (a toggle toggles there; a tool used over a Visual selection repeats over as many beats or bars from the cursor; a tool that asked for an argument, like `:direction`, with the same one) |
| `u` / `U` / `g-` / `g+`                                | undo / redo / older / newer state across undo branches                                            |
| `q{reg}` … `q` / `[count]@{reg}` / `@@`                | record / play / replay a macro; `q{A-Z}` appends to the lowercase register                        |
| `v` / `V`                                              | visual beats / visual bars (then `d`, `y`, `c`, `x`; `i` / `a` below)                            |
| `gv`                                                   | reselect the last Visual selection of this file (same kind; in Visual mode: swap with the current one) (`:visual-reselect`) |
| `+` / `-`                                              | longer / shorter note value                                                                       |
| `<C-a>` / `<C-x>`                                      | pitch up / down a semitone (moves string when the fret runs out)                                  |
| `r`                                                    | make the beat a rest                                                                              |
| `t.` / `t3`                                            | toggle dot / triplet                                                                              |
| `ZZ`                                                   | write and close                                                                                   |

The `t` keys depend on the notation under the cursor (`[keymap.normal-tab]`, `-standard`,
`-slash`, looked up before `[keymap.normal]`; `<F1>` and the keymaps picker list the
current ones):

| notation | keys |
| --- | --- |
| tab | `th` `tb` `ts` `tv` `tx` `tm` `tl` `tg` `ta` `tn` `tt`: toggle hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie |
| standard | `th` `ts` `tv` `tx` `tg` `ta` `tt` (hammer/pull, slide, vibrato, dead, ghost, accent, tie), `t#` `t&` `t=` (sharp, flat, natural) |
| slash | `ta` `tt` (accent, tie) |
| chant score | its own set, `[keymap.normal-chant]`: see "Chant" below (`tp` `tv` `tq` `tm` `te` `tb` `td;` `tc3` …); chant signs can be typed in Insert mode too |

**Visual mode** (`v` beats, `V` bars; a `d` `y` `c` `x` act on the selection, motions extend it):

| keys | command |
| --- | --- |
| `+` / `-` | step the duration of every selected beat one note value longer / shorter (dots and tuplets stay; a whole note is the longest; the status line says when a bar runs over) (`:duration-longer`, `:duration-shorter`) |
| `t…` `H` `L` … (the Normal-mode technique keys of the staff), the palette | the range tools below, on every beat of the selection (the selection stays, so several can be applied; `<Esc>` leaves) |
| `o` | move the cursor to the other end of the selection (`:visual-other-end`) |
| `i` / `a` | leave the selection and insert a new beat before its first / after its last beat (a bar selection: at the start of its first bar / the end of its last bar), then Insert mode (`:visual-insert`, `:visual-append`) |

**Selecting with the mouse** (GUI and terminal): press on a beat of a score pane and drag across
other beats to select them. A press places the cursor as a click does, and nothing else happens
while the pointer stays on that beat; once it moves to another beat, Visual mode starts with the
anchor at the pressed beat and the cursor end follows the pointer live. Releasing keeps the
selection in Visual mode, so `d` `y` `c`, the palette's buttons and the other keys act on it. The
drag runs through bars and, down the page, over the systems; in the GUI, holding the pointer
near (or beyond) the pane's top, bottom, left or right edge scrolls the view, faster the further
out, and the selection follows. A Visual selection covers the beats of the cursor's own track and
voice, so a drag selects on the track pressed: pointing at another track's staff selects those
bar and beat positions of the pressed one (the cursor keeps its string or line). `Shift`+click
extends the selection from the anchor (from the cursor, entering Visual mode, when not in it) to
the clicked beat, and goes on as a drag. `Ctrl` or `Alt` with the press (or drag, or `Shift`+click)
selects whole bars, as `V`; a double click selects the bar. A plain click in Visual mode moves
the selection's cursor end, as before (a drag from it starts a new selection at the pressed
beat). A drag from Insert mode leaves it for Visual mode; `<Esc>` during a drag ends the
selection. Design-mode handles, scrollbars, borders and the tool panes take their presses first,
and a drag started on them never selects.

Choices where the wish was open: `e` is vim's end-of-word, so on a note that does not start a
tie it goes to the end of the *next* note's tie (a lone note is its own tie); `b` likewise to
the start of the previous one. `ch` clears the beat before the cursor and the cursor goes there.
`dl` / `cl` / `yl` on the last beat of the score still act on that beat. `^` / `$` ignore the
row (a `w` on a string would not find the first note of a bar played on another string).
With `d` and `c`, `e` `$` are inclusive, `w` `b` `^` `h` `l` exclusive, as in vim. `cc` and the
other changes clear only the cursor's track (a bar's other tracks stay); `dd` and `yy` still
act on the bar of every track, as bars are shared.

### Other

| keys        | command          |        |
| ----------- | ---------------- | ------ |
| `:`         | command line     | _(M1)_ |
| `<F1>`      | help: the key bindings of the current mode (Normal, Insert, Visual, command line, picker, tracks/properties/palette pane); `<F1>` or `<Esc>` hides it. Works in every mode; in the Normal modes a user binding of `<F1>` takes precedence |        |
| `<leader>?` | find a key binding (keymaps picker) |        |
| `ZZ`        | write and quit   |        |
| `<C-q>`     | quit cecilia (`:quit`; not while files have unsaved changes) | |

## Status line

As lualine.nvim: on the left the mode (in its color), the pane's layout (in the mode's color),
the workspace and the file (● when changed); on the right the tempo, play time, play-along
settings, macro recording, insert note length and zoom, then the position as `Top` / `NN%` /
`Bot` and `bar:beat` in the mode's color. Beside the mode is a chip naming the kind of score,
`MODERN` (theme color `score_modern`) or `CHANT` (`icon_chant`); click it for how to convert the
score (`:chant-to-modern` / `:modern-to-chant` open a new buffer, so it is not done by a click).
The bufferline shows the chant icon on a chant score's tab and the score pane's title names the
kind (`t.gabc — chant`). Click the layout to cycle it, the workspace to pick
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
solarized-dark, solarized-light, catppuccin-latte, -frappe, -macchiato and -mocha, and the logo-colored cecilia-dark, cecilia-hybrid — dark UI, light
page — and cecilia-light), an optional theme file of your own (`file`), and any color by
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
| `:track-color #rrggbb` / `:track-color off` / `:track-color` | the track's color (saved in `.cc` files) / back to the theme's / the next palette color |
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
  instructions) and *Page* (page size, margins and scale, as Guitar Pro 5 files keep them; cecilia
  doesn't print with them yet). Tempo and the pickup bar are edited in the score itself. The rows
  depend on the kind of score; rows that don't apply are not shown. A **chant** score shows its
  GABC header fields instead of the meta data (`name`, `office-part`, `mode`, `annotation`, `book`,
  `transcriber`, `commentary`, `user-notes`, then any other header the file has, in order; an empty
  value leaves the header out), *Chant* (the interpretation, equalist or Solesmes, and the number
  of staff lines, 2–5) and *Page*. Each edit is one undo step.
- **TRACK** — the track under the score cursor (`n`/`p` change it): *Info* (name, kind, color),
  *Notation* (tuning, frets, capo, and the key *Staves* over one field per staff, top to bottom, with an empty "Add Staff" field at the bottom; each opens a picker of staff kinds, shown with an icon, and a field's handle drags it to another place, a line showing where it lands; the staves are modern tracks only) and *Sound*
  (instrument, MIDI channel, bank, volume, pan, mute, solo). A **chant** track shows name and color,
  *Notation* with its initial *Clef* (opens the clef picker) and *Sound* without channel and
  bank; no kind, staves, tuning, capo or frets. (Playback transposition of a chant is not
  available yet.)
- **LYRICS** — the cursor track's lyrics as one text area (below; full reference in
  [LYRICS.md](LYRICS.md)), the single source of the syllables on its notes. The text is shown
  and edited as written. The text area has no label, box or background; it is in the app's text
  font (GUI: `ui.lyrics_font`, any installed family, at `ui.lyrics_font_size`, 12 pt) and colored
  by the theme: words `lyrics_text`, `[labels]` `lyrics_label` in bold, `(bar:beat)` anchors
  `lyrics_anchor`, markup tags `lyrics_markup`. A chant score's lyrics are its chant: its text
  area always shows the GABC body of the chant (its header is kept; clefs, notes and groups in
  the `lyrics_clef` … colors), read back as GABC when edited.

### Lyrics syntax

`some-one` hyphens, `some -- one` and `some __ one` dash and skip words (two more beats each),
`<i>si</i>-lence` GABC markup tags (`<i>` `<b>` `<ul>` `<sc>` `<c>`) for styles, `[1.]` labels,
`*` printed ✻, `(bar:beat.fraction)` anchors (as the status line counts). Syllables go one
to a note in the order the music is played (repeats, alternate endings, D.C./D.S.), the second
pass of a repeat on the second verse row. The syntax, worked examples, placement rules and every
key of the text area are in [LYRICS.md](LYRICS.md).

Every edit is an ordinary, undoable score edit. `<leader>P` focuses the pane (again: back to the
score pane). It stays Normal mode; while the pane has the focus its keys are
`[keymap.properties]`, then the usual ones:

| keys | command |
| --- | --- |
| `j` `<Down>` `<Tab>` / `k` `<Up>` `<S-Tab>` | select the next / previous property; selecting the lyrics text area gives it the caret, in normal mode (a block) |
| `gg` / `G` | select the first / last property |
| `zz` | scroll so the selected property is in the middle of the pane (`:properties-center`) |
| `L` `<C-Tab>` / `H` `<C-S-Tab>`, `1` `2` `3` | next / previous tab, the score / track / lyrics tab |
| `<CR>` `i` `a` | edit the property (insert mode). The score updates as you type, and the edit is one undo step. `<CR>` keeps a single line, `<Esc>` and `<C-c>` cancel it; `<Left>` `<Right>` `<Home>` `<End>` move, `<C-u>` clears. The lyrics text area is edited like vim: `<Esc>` goes to normal mode (`h` `j` `k` `l` `w` `b` `e` `W` `B` `E` `0` `^` `$` `gg` `G` move; `d` `c` `y` over motions and text objects (`dw` `diw` `daw` `dap` `ci"` `yip` …), `dd` `x` `D` delete; `v` `V` Visual mode; `i` `a` `I` `A` `o` `O` `<CR>` insert; `u` `<C-r>` undo and redo; `H` `L` change tab; a focus key such as `<C-h>` leaves the text area, and focusing the pane again restores the caret; in insert mode `<C-Left>` `<C-Right>` jump words, `<C-BS>` `<C-Del>` delete by word; see LYRICS.md for all), each change and each insert session an undo step; `<CR>` in normal mode is `i`; `<Esc>` in normal mode leaves the text area and the pane. While it has the caret the text area takes every key (`<Esc>` first for `:` and `<leader>` keys); in insert mode `<C-u>` clears all of it and `<C-c>` copies. The status line shows the current mode. Color opens a picker of named colors, each with a swatch and its hex code; type `#rrggbb` or `rgb(r,g,b)` for your own. Tuning opens a picker of common tunings of the track's instrument (Drop D, DADGAD, Open C …), searchable by name or description; type the notes (`D2 A2 D3 G3 B3 E4`) for your own. Instrument opens its picker; mute and solo are switches (a click flips them, with on/off beside them); a section header folds |
| `<A-h>` / `<A-l>` | step the selected property down / up: a switch off / on; a choice (color, tuning, instrument, a staff's notation or clef) the previous / next one, without wrapping; a number down / up by one |
| `c` / `d` | clear the property's text and edit it / just clear it (in the lyrics text area these are its own vim keys; `:properties-clear` there clears the lyrics) |
| `l` `<Right>` / `h` `<Left>` | on a staff row, select the clef box / the notation box (a click on a box selects it too); tab and slash staves have no clef |
| `<A-j>` `<A-k>` | swap the selected staff with the one below / above |
| `za` `zc` `zo` | toggle / close / open the section |
| `x` | same as `d`: clear the text or the track's own color, or remove the selected staff |
| `/` | search: a field opens at the bottom of the pane and jumps to the first key or value with what is typed (any case, unless a capital is typed); `<CR>` keeps the jump and sets the `/` register, `<Esc>` goes back |
| `n` `N` | jump to the next / previous match of the `/` register (in the lyrics text area, normal mode: through the text, `/` too) |
| `<Esc>` `q` | back to the score pane |
| `<leader><leader>` `<leader>s` `<leader>f` `:` | play/pause, stop, follow, command line |

Clicking a tab shows it, a row selects it and a double click edits it; a click on a switch flips it, and a click in the lyrics text area gives it the caret there, in normal mode. Instruments and staves are shown with an icon (a guitar for every kind of guitar, and so on). In the lyrics text area being edited the mouse works as in any text field: a click places the caret, a drag selects (past the top or bottom it scrolls), a double click in insert mode selects the word (in normal mode it enters insert mode at the click), typing replaces the selection, `<BS>`, `<Del>` and in normal mode `d` `x` remove it and `c` changes it. `<C-a>` selects all; `<C-c>` copies, `<C-x>` cuts and `<C-v>` pastes through the system clipboard (`wl-copy`/`wl-paste`, `xclip`, `xsel`, `pbcopy`/`pbpaste` or PowerShell, whichever is there); in normal mode `y` copies a selection, `yy` the line, and `p` / `P` paste after / before the caret. The wheel scrolls the pane and moves the selection in pickers. In the lyrics text area `j` and `k` move by the lines as wrapped on screen.

| command | |
| --- | --- |
| `:properties [on\|off]` / `:properties-focus` | show or hide the pane / focus it |
| `:properties-tab [score\|track\|lyrics]` | show a tab (none: the next) |
| `:properties-first` / `:properties-last` | select the first / last property |
| `:properties-fold [open\|close]` | open, close or (no argument) toggle the selected section |

## Editing palette

`<leader>e` shows or hides the editing palette, docked along the left of the window (the bottom tool panes run under the scores only, between it and the properties pane; the tracks pane alone runs along the bottom of the whole window)
(`ui.palette = true` shows it from the start). Like Guitar Pro's editing panel it is a grid of
small icon buttons, wrapped to the pane's width, in collapsible sections: **Duration** (whole to
64th, dotted, double dotted, triplet, rest), **Pitch** (sharp, flat, natural, double sharp, double
flat, octave, tie, dead and ghost notes), **Dynamics** (`ppp` to `fff`, crescendo, decrescendo,
fade in, fade out, volume swell), **Articulation** (accent, staccato, fermata, let ring, palm
mute), **Effects** (legato = hammer-on/pull-off, bend, vibrato, harmonic, artificial harmonic,
tremolo picking, tap, left-hand tapping, slap/pop, grace note before / on the beat), **Slides**
(shift, legato, in from above / below, out downwards / upwards), **Strokes** (down / up stroke,
arpeggio down / up, rasgueado, pickstroke down / up), **Beams & stems** (automatic, force, break,
break secondary; automatic, up, down), **Bar** (clef, key signature, time signature, free time,
double bar line, section) and **Repeats** (repeat one / two bars, open, close, alternate endings,
directions). Buttons marked "…" in their name (clef, key and time signature, section, alternate
endings, directions) need an argument: they open the command line with the command typed
(`:clef `, `:direction `) and its completion, so they work from the keyboard. The
GUI shows the highlighted button's name along the bottom of the pane (and as a tooltip when the
mouse rests on it); the terminal draws short labels and shows the name on the pane's bottom row. Each button runs an ordinary command
(`:duration 8`, `:toggle palm-mute`, `:dynamic mf` …) on the score pane worked on. The buttons in
force at the cursor (the beat's duration, its dynamic and techniques) are tinted with the accent
and follow the cursor as it moves; the highlighted button is a neutral gray. The mouse and the
keyboard share that one highlight: moving the mouse over a button selects it. `<leader>E` focuses the pane (again: back to the score); while it has the focus its keys
are `[keymap.palette]`, then the usual ones:

The palette's sections depend on the kind of score under the
cursor and change when you switch buffers; the folds, selection and scroll are remembered for each
kind. A **chant** score has: *Clef* (do 1–4, flat do, fa 1–4), *Divisions* (`,` `;` `:` `::`, the
`;1`…`;6` variants), *Shapes* (punctum, inclinatum, virga, virga reversa, bivirga, stropha, quilisma,
oriscus, quadratum …), *Liquescence*, *Rhythm* (mora, episema, ictus), *Alteration* (flat, natural),
*Pitch* (up / down a staff position) and *Text* (`*` `+` `℣` `℟`); each item runs a `:chant-*`
command. In the GUI the flat and natural show their signs and the other chant items their short
labels, as in the terminal. The modern sections follow.

| keys | command |
| --- | --- |
| `h` `l` `<Left>` `<Right>` | select the button to the left / right; past the end of a line it goes on to the next / previous line of buttons, and around the ends of the grid (`ui.palette_wrap`; off: stops at the ends of the line) |
| `j` `<Down>` `<Tab>` / `k` `<Up>` `<S-Tab>` | select the button below / above (keeping the column), through the section titles, wrapping from the last line to the first and back (`ui.palette_wrap`) |
| `gg` / `G` | select the first / last row of the palette (the top / bottom; sections folded away and tools not shown for the mode and staff are not there), scrolling it into view; the score's cursor and selection stay (`:palette-first`, `:palette-last`). While the palette has the focus `g` is a prefix of `gg` only |
| `<CR>` `<Space>` | apply the selected button (on a section title: fold or unfold it) |
| `za` `zc` `zo` | toggle / close / open the section |
| `/` | search the tools (`:palette-search`): a field opens along the bottom of the pane and the selection jumps to the first tool (or section) whose name, tooltip (with its keys), command or section title has the typed text, as you type; lower case matches any case, a capital is exact. `<Esc>` goes back to where the search began, `<CR>` keeps the jump (and sets the `/` register; it does not apply the tool), `<Backspace>` on an empty field cancels, `<C-u>` clears it |
| `n` `N` | the next / previous match of the `/` register (`:palette-search-next`, `:palette-search-prev`), wrapping around |
| `<Esc>` `q` | back to the score pane |

The palette lists only the tools that apply to the mode and to the staff under the cursor (a
section with none left is hidden; folds are kept): on a **tab** everything but the accidentals;
on a **standard** staff the accidentals and the marks it can show (no bends, palm mute, let
ring, tap / slap / pop, fades or left-hand tapping); on a **slash** staff the note values, rest,
tie, accents, staccato, dynamics, hairpins, fermata and beams; on **chant** the note values and
the clef; in **Visual** mode (see below) every tool that makes sense over a range, for the
staff's notation. The bar and repeat tools (everything but the clef) apply on every notation but
chant tracks; the clef on standard staves and chant tracks, stems on standard staves only. It follows the
cursor from one staff to another.

**Over a Visual selection** (`v` beats, `V` bars) the palette's tools, `+` / `-`, and the same
commands typed (`:toggle staccato`, `:dynamic p` …) apply to every beat of the selection in one
undo step, and a button is tinted when the whole selection has it:

- Moving between the score and the palette keeps the selection: `<leader>E` focuses the palette (the mode stays Visual); while it has the focus `h` `j` `k` `l` and the arrows move in its grid, not the selection, `<CR>` applies the selected tool to the selection, `/` searches, and `<Esc>` (or `<leader>E` again) gives the focus back to the score, where `hjkl` extend the selection again.
- Moving between the score and the palette keeps the selection: `<leader>E` focuses the palette (the mode stays Visual); while it has the focus `h` `j` `k` `l` and the arrows move in its grid, not the selection, `<CR>` applies the selected tool to the selection, `/` searches, and `<Esc>` (or `<leader>E` again) gives the focus back to the score, where `hjkl` extend the selection again.
- Techniques (`:toggle`: staccato, accent, marcato, let ring, palm mute, vibrato, harmonics,
  tremolo, tap / slap / pop, strokes, arpeggio, rasgueado, pick strokes, fermata, fades, grace
  notes, left-hand tapping …): if every beat or note has it, it is taken off all; otherwise it
  is put on those that lack it.
- Crescendo / decrescendo: one hairpin from the first selected beat to the last (consecutive
  beats with the same hairpin are drawn as one wedge); again takes it off.
- Dynamics: `ppp` … `fff` set every note of the selection to that dynamic (rests are left
  alone); one mark is engraved at the start of the section and another where the dynamic differs
  again, so the beat after the selection keeps and shows its own. "Louder" / "softer" in the
  palette, `<A-+>` / `<A-->` in Visual mode (`:dynamic-step +1`, `:dynamic-step -1`; also on the
  cursor note outside Visual mode) move every note one dynamic up or down, keeping the
  differences between them until they reach `ppp` or `fff`.
- Legato (hammer-on / pull-off), legato slide and shift slide link each beat to the next, so the
  last beat of the selection is left out (a single beat is just toggled); ties join each note to
  the note before it on the same string (or of the same pitch).
- Note values: a value sets every beat, dot / double dot / triplet toggle on all (a triplet over
  three selected beats), `+` / `-` step longer / shorter, rest turns the selection into rests.
- Pitch: semitone up / down, octave up / down, and the accidentals (also double) over the
  selection; notes that would leave the neck stay and are reported.
- Beams (auto, force, break, break secondary: `force` leaves the first beat alone) and stems.
- With **bar** selections (`V`) the bar tools too: repeat open, section, key and time signature
  at the first bar; repeat close, double bar and direction at the last; free time and alternate
  endings on every selected bar. The clef and bar repeat signs stay out.
The duration selected in it (the tinted button) is the length of the beats `i`, `a`, `I`, `A`
and Enter in Insert mode make.

A button's name (tooltip and label) ends with the insert-mode keys bound to its command, from the live keymaps so your overrides show ("Palm mute (m)", at most three, "Quarter note (<A-3>)"; none: no parentheses). Buttons that can't take effect at the cursor are dimmed and named "(unavailable)" after that: note-level ones (pitch, tie, dead and ghost notes, dynamics, articulations, most effects) off a note, accidentals on a tab. They are worked out by trying the command's own edit, so they follow what the command refuses. They stay selectable, but applying one does nothing and the status line says why.

`.` repeats the last tool applied from the palette (or by a key over a Visual selection) as long as no editing key changed something after it: it runs again at the cursor, as one undo step. Moving about or searching in the palette, or a tool that changed nothing, leave what `.` repeats alone. Tools typed on the command line (`:toggle staccato`) are not repeated, except those begun from a palette button that asks for an argument.

Search is the properties pane's (`/`, `n`, `N`), and only looks through the tools shown at the moment: those of the staff and mode, in sections not folded. The buttons' icons are the engraver's own symbols in the GUI; the terminal draws the closest Unicode or Nerd Font glyph (`tabline.icons`: `nerd`, `unicode`, or `none` for short text labels).

A click applies a button (and returns to the score pane) or folds a section; the wheel scrolls.

| command | |
| --- | --- |
| `:palette [on\|off]` / `:palette-focus` | show or hide the pane / focus it |
| `:palette-down` `:palette-up` `:palette-first` `:palette-last` `:palette-left` `:palette-right` `:palette-activate` `:palette-fold [open\|close]` | move, apply, fold |
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
staff, on a **slash** staff or on a **chant track**. Each has its own keymap, looked up first
(`[keymap.insert-tab]`, `[keymap.insert-standard]`, `[keymap.insert-slash]`,
`[keymap.insert-chant]` in `config.toml`), then the keys shared by all (`[keymap.insert]`).
The status line shows where the cursor is (`string 3`, `G4`, `slash`, `do`, `re` … `ti` on chant) and,
in every mode, how full the cursor's bar is: the beats of the voice under the cursor against what the time signature asks
for, in quarter-note beats (`3.0/4.0` needs one more quarter; 6/8 is `3.0`, 7/8 `3.5`). One
decimal, two when that is not exact (a triplet remainder reads `3.67/4.0`). A complete bar
reads in the normal color, a short or over-full one (`4.5/4.0`) in the error color. Free-time
bars (chant) and a pickup first bar have no required count and show nothing. With
`ui.highlight_incomplete_bars` (`:set ui.highlight_incomplete_bars toggle`) such bars also get red staff lines.

Shared keys:

| keys                         | command                                                         |
| ---------------------------- | --------------------------------------------------------------- |
| `<Space>`                    | next beat, appending one (or moving to the next bar) at the end |
| `<Left>` `<Right>` / `<Up>` `<Down>` | the same as `h` `l` / `k` `j`: previous / next beat (no beat is added), row up / down (across staves). On chant and on drum tracks, where those letters are pitches and instruments, the arrows still move |
| `h` `l` / `j` `k`            | previous / next beat (no beat is added), row down / up. Not on chant, where they are pitches; on drum tracks `h` and `k` are the hi-hat and kick |
| `<CR>`                       | finish the beat: on a tab the fret typed stays (an empty beat becomes a rest); on a standard staff a note is put at the cursor's line or space (in the key; nothing if the beat has a note there already); on a slash staff the slash. Then the cursor goes to the next beat, as `<Space>` (appending one while the bar has room for it, else to the start of the next bar, adding a bar at the end of the score). The beats get the insert duration: the duration tool of the palette, `+` `-` `<A-1>`…`<A-7>`. On a chant score `<CR>` puts a note in the empty beat the cursor is on, see "Chant" |
| `<A-CR>`                     | new beat after the cursor, staying (`:beat-new`; the old `<CR>`) |
| `<A-v>` / `<A-V>` / `<A-F1>`…`<A-F4>` | next / previous voice, voice 1…4 (see Voices) |
| `<BS>`                       | delete the note                                                 |
| `<Del>`                      | delete the whole beat, leaving no rest (`:delete-beat`; chant: the note) |
| `r` / `.` / `<C-t>`          | rest / dot / triplet                                            |
| `+` / `-` / `<A-1>`…`<A-7>`  | longer / shorter / whole … 64th                                 |
| `<C-a>` / `<C-x>`            | the cursor note a semitone up / down                            |
| `<Esc>`                      | back to Normal (the whole session is one undo step)             |

On a **tab**:

| keys                                        | command                                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `0`–`9`                                     | fret on the cursor string (two quick digits = two-digit fret)                              |
| `H` `b` `s` `~` `x` `m` `L` `g` `a` `n` `t` | hammer/pull, bend, slide, vibrato, dead, palm mute, let ring, ghost, accent, harmonic, tie (`H` and `L` were `h` and `l`) |

On drum tracks _(M7)_, letters add or remove an instrument on the cursor beat: `k` bass drum,
`s` snare, `h` closed hi-hat, `o` open hi-hat, `p` pedal hi-hat, `c` crash, `d` ride, `t` high
tom, `m` mid tom, `f` floor tom (`r` is still a rest, `g`/`a` ghost and accent).

On a **standard** staff the cursor sits on a line or space (`<Up>`/`<Down>` move it a step; past
the staff they go on to the tab below or the next staff):

| keys                                  | command                                                                                                                                                   |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a`–`g`                               | that note, in the key, nearest the cursor (`:note-letter`): it replaces the note the cursor is on, or joins the chord; the cursor moves onto it. Fretted tracks pick a string |
| `#` / `&` / `=`                       | the cursor note sharp / flat / natural (`:accidental`)                                                                                                    |
| `t` `H` `s` `~` `>` `(` `x`           | tie, hammer/pull, slide, vibrato, accent, ghost, dead                                                                                                     |

On a **slash** staff: `/` puts a slash on the beat (`:slash`; `<CR>` does as well); `t` tie, `>` accent.

On a **chant score** (opened from GABC; there is no chant staff on modern tracks), the cursor is
on a *cell*: a note, an empty beat `()` or a division written as its own symbol `(:)` (a single
square on that position of the chant staff). The chant is what is edited; the
bars of a chant track are only a transcription made from it, so every key below changes the chant
(one undo step each), the bars are written again and the cursor stays on the edited note.

| keys                                      | command                                                                         |
| ----------------------------------------- | ------------------------------------------------------------------------------- |
| `h` `j` `k` `l`                           | move the cursor, as on a standard staff (beat back / position down / up / beat on) |
| `i` `a`                                   | a new empty beat `()` before / after the cursor cell, then Insert mode on it (an empty beat you leave without entering anything is taken away again by `<Esc>`) |
| `I` `A`                                   | a new empty beat at the start of the bar (phrase) / at its end, before the division that closes it |
| `<CR>` `<Space>` (Insert, modern scores)  | finish the beat the cursor is on (a standard staff: a note on the cursor's line, a slash staff: a slash; a tab's fret is already in) and add an empty beat after it; the cursor stays on the beat, `hjkl` move on. A bar that is full gets no empty beat. `<Esc>` takes away the empty beat nothing was entered in (`:insert-enter`, `:insert-next`) |
| `<CR>` `<Space>` (Insert, chant)          | a punctum on the staff line the cursor is on goes into the empty beat the cursor is on (after the cursor cell otherwise), and a new empty beat follows it; the cursor stays on the note (`l` goes on to the empty beat for the next note). On a note: another staff line moves it there keeping its modifiers, its own line adds another note of that pitch after it in the same group (`(g)` → `(gg)`). `<Esc>` drops the empty beat after the cursor. A group entered this way is written without a space before it; spaces come with the lyrics (`:lyric-mode`) (`:insert-enter`, `:insert-next`) |
| `R` (`:replace-mode`)                     | Replace mode (also on modern scores, where it is Insert mode without the empty beats): as Insert mode, but no empty beats are added after the notes entered. `<CR>` and `<Space>` move the note under the cursor to the cursor's line, keeping its modifiers, or, on its own line, add another note of the same pitch after it in the same group: `(g)` → `(gg)` → `(ggg)`; in an empty beat they put a note in. The cursor stays. `<Esc>` leaves |
| `<A-CR>` (Insert) | a new empty beat after the cursor cell, entering nothing (`:beat-new`) |
| `/` `!`                                   | toggle a neume separator `/` or `!` after the cursor note, inside its syllable: `foo(ed)` → `foo(e/d)` (`:chant-sep /`, `:chant-sep !`; another separator there is replaced). `:chant-break` adds a note after the cursor note starting a new neume; `:chant-append`, `:chant-syllable` add a note in the same neume / a new syllable |
| `<A-i>` (`:lyric-mode`)                   | lyric mode: Insert mode where the keys type the cursor syllable's text (`<BS>` deletes, `<Tab>` / `<S-Tab>` go to the next / previous syllable with notes, `<Esc>` or `<CR>` leaves); the score and the lyrics text area follow every key, and the mode is one undo step. `:lyrics TEXT` sets the cursor syllable's text to all of TEXT |
| `<S-Del>` (`:lyrics-delete`)              | delete the lyrics (the text) of the syllables of the Visual selection, or of the cursor syllable, keeping the notes; one undo step |
| `:lyrics-shift N`                         | move the notes N syllables on, or back when negative, among the syllables of the Visual selection (else from the cursor syllable to the end of the chant), the lyrics staying where they are. Clefs and divisions of their own stay; notes pushed past the last syllable come round to the first, so an empty group moves to the other end: `be-() ne-(i) díc(g')` with -1 is `be-(i) ne-(g') díc()`. Not bound to a key |
| `:chant-markup TAG`                       | `<TAG>…</TAG>` (`i` `b` `ul` `sc` `c` `tt` `e` `alt`) goes around the cursor syllable's text, or comes off when it is there; the Text section of the palette has a tool for each |
| `<A-J>` / `<A-S>` (`:chant-join` / `:chant-split`) | join: the syllables of the Visual selection (else the cursor syllable and the next one with notes) become one, the notes going to the first and the text staying: `foo(e) bar(d)` → `foo(ed) bar()`. Split: the syllable splits after the cursor note (after each selected note): `foo(ed) bar` → `foo(e) (d) bar`. Bound to `<A-J>` / `<A-S>` (Alt+Shift) in Normal, Insert and Visual mode (in Visual they act on the selection) |
| `<C-a>` `<C-x>`                           | the note up / down one staff position (`:pitch-up`, `:pitch-down`); `:octave-up` / `:octave-down` move seven. A pitch moves by position, not semitone; the top and bottom of the staff stop it |
| `v` `s` `o` `q` `w` `~`                   | virga, stropha, oriscus, quadratum, quilisma, liquescent (toggles, `:chant-mod`; `:chant-mod ss` / `vv` / `V` / `<` / `>` also) |
| `G` `V` `O` `<` `>` `r` `+` `x` `y` `#` (Insert mode) | the same toggles as the Normal-mode `t` keys, by GABC letter: inclinatum `G`, virga reversa `V`, oriscus with stem `O`, liquescent ascendens `<` / descendens `>`, cavum `r`, custos `+` (`:chant-custos`: the note becomes a custos, `g` → `g+`, and is not a note any more), flat `x`, natural `y`, sharp `#` (`:chant-alter`); with `v` `s` `o` `w` `q` `~` they follow the GABC shape table. On an empty beat a punctum goes in first |
| `z` `Z` (Insert mode)                   | a forced line break after the cursor syllable goes in or out: `z` breaks the line, `Z` breaks it without stretching the line to the width (`:chant-linebreak z`, `:chant-linebreak Z`) |
| `.` `_` `'` `-`                           | mora, episema, ictus, initio debilis (`:chant-mod`; `:dot` is the mora too)     |
| `:accidental flat` / `natural` / `sharp`  | the flat, natural or sharp before the note (a second time takes it away)        |
| `+` `-` (normal mode)                     | horizontal episema on / off (`:duration-longer`, `:duration-shorter`)           |
| `,` `;` `:`                               | a division (`:chant-bar`): on a note it goes into the note's own parentheses, `(g::)`; on an empty beat it is a division of its own, `(:)` (written with a space before it and a line break after it in the GABC text, which go with it when it is taken off; in Insert mode an empty beat follows it, as after a note); on a division of its own it replaces that one. `,` and `;` again take it off, `:` goes round single `:`, double `::`, none |
| `x` `<BS>` `<Del>`, Visual `d`            | delete the cell (the cells of the selection): a note, an empty beat or a division of its own; a syllable left with no text and no notes goes. The cursor goes to the cell before |
| `h` `l`                                   | in Normal mode, the previous / next cell, the cursor standing on the staff line of a note it lands on (on an empty beat or a division it only moves along); in Insert mode they just move left / right and the staff line stays, so `j` `k` choose the pitch of the next note entered |
| `w` `e` `b`                               | next note / end of the note / previous note *on the cursor's staff position* |
| `<leader>g` (`:lyrics-select`)            | jump to the lyrics text area with the caret on the cursor note's pitch letter in Normal mode (`ui.lyrics_select_cursor`, on by default, does so whenever focus goes from the score to the lyrics pane, as `<leader>P`) |
| `<Right>`                                 | next note                                                                       |

Everything that
edits the bars of a modern track reports **"not in chant"** and changes nothing: durations
(`:duration`, `+` `-` `<A-1>`…`<A-7>` in insert mode), tuplets, rests, ties and the other
`:toggle` effects, dynamics, beams, stems, `:note-letter`, double sharps and flats, bar insert /
open / delete, `p` `P` paste, Visual `c`, `:reflow`, and digits (no frets). The same refusal covers
any other edit that would change the bars of a chant track without changing its chant.

## Command mode (selected)

`:` opens the **command palette**, a box near the top of the window (in the terminal UI too).
As you type, the matches for the word being typed are listed below it: command names with a short
description on the right, then each command's arguments — file names for `:open`, `:w`,
`:view-new` and `:audio` (folders first, only files cecilia can open or write) and the values of
`:layout`, `:mix`, `:volume`, `:track-add`, `:tuning`, `:toggle`, `:duration`, `:clef`,
`:interpretation`. `<Tab>` fills in a unique match (or the common part), and `<Tab>`/`<S-Tab>`
or `<Down>`/`<Up>` move through the list, putting the selected match on the line. `<Enter>`
runs the line, `<Esc>` closes the palette, `<C-p>`/`<C-n>` recall earlier commands.

Matching is **fuzzy** by default: the typed letters may appear anywhere in a candidate, in order
(`:sd` finds `sync-detect`, `speed-down`), with prefix matches first, then matches at word
starts and in runs; matched letters are highlighted. One candidate starting with what you typed
still completes outright on `<Tab>`. Matching also ignores letter case. Turn either off with
`ui.completion_fuzzy = false` / `ui.completion_ignore_case = false` in `config.toml`, or on the fly
with `:set ui.completion_fuzzy toggle` and `:set ui.completion_ignore_case toggle`; bind them
under `[keymap.command]` to flip them while typing, e.g.
`"<C-g>" = "set ui.completion_ignore_case toggle"`, `"<C-f>" = "set ui.completion_fuzzy toggle"`.

### Options: `:set` and `:get`

Every setting of `config.toml` (except the keymaps) is an *option*, named by its dotted TOML path
in lower_snake_case (`ui.tab_width`, `tabline.wrap`, `theme.preset`) and always has a default.
Options are set in `config.toml`, on the command line with `-o NAME=VALUE ...` (put `-o` after
the subcommand, or end the options with `--` before the files), and in the app with
`:set NAME [VALUE]`; `:get NAME` shows the current value. `<Tab>` completes names and values.

- No value, or `default`: reset to the default. A `:set` or `-o` value lasts for the session and
  survives a reload of `config.toml`; `:set X default` pins the default over the file.
- **boolean**: `true` / `false`, or `toggle`.
- **number** (integer or float): `N` sets, `+N` / `-N` add to the current value. Out of range
  values are refused, naming the range.
- **string** (including colors and choices): the rest of the line, verbatim
  (`:set ui.focus_ring_color #fabe28`; an empty color means the mode's color).
- **array**: a comma-separated list with no spaces (`:set ui.global_tools tracks,tuner`);
  `+item` adds, `-item` removes (`:set ui.global_tools +tuner`). A leading `+` / `-` applies to
  the whole value (`+a,b` adds both).

An invalid value changes nothing and the error lists what is allowed.

| command                                                              |                                                                                                                                                                                             |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `:open {file}` / `:w [file]` / `:q` (quit) / `:wq` (write all, quit) / `:q!`                  | _(M2)_ `:w file.ext` exports by extension                                                                                                                                                   |
| `:save-as [file]` (`:saveas`) | write to a new file and make it the buffer's file (later `:w` writes there; `:w file` stays an export). No file: the system save dialog (starting beside the current file, suggesting its name or `untitled.ccm` (`.ccc` for a chant); cancelling does nothing; no file extension means `.ccc` for a chant score, `.ccm` otherwise). A plain `:w` on a buffer with no file runs it too. Builds without the `dialog` feature say "use :write FILE" |
| `:wa` (`:wall`) / `:pane-close` | write every changed file / close the focused pane (its view with the last pane); the last pane of the last view cannot be closed (a message says so; close its buffers to reach the [dashboard](#dashboard)); the remaining panes of its split share the space equally again (`ui.equalize_on_close`; off: the neighbour takes it) |
| `:split` / `:vsplit`                                                 | _(M1)_                                                                                                                                                                                      |
| `:track {n}`                                                         | _(M1)_                                                                                                                                                                                      |
| `:bar {n}` / `:{n}`                                                  | _(M1)_                                                                                                                                                                                      |
| `:set NAME [VALUE]` / `:get NAME` | change / show an [option](#options-set-and-get) for the session |
| `:source`                                                            | reload config _(M1)_                                                                                                                                                                        |
| `:time 3/4` / `:key -2` / `:key Bb` / `:key F#m` / `:tempo 96`       | time / key signature from the cursor bar (the key by sharps or flats, or a name; `Am`, `C minor` are minor), tempo at the cursor bar                                                         |
| `:repeat-start` / `:repeat-end [n]` / `:section NAME` / `:text TEXT` | bar marks and beat text                                                                                                                                                                     |
| `:free-time` / `:double-bar`                                         | toggle free time / a double bar line on the cursor bar                                                                                                                                      |
| `:bar-repeat 1\|2\|off`                                              | the cursor bar (or it and the next) repeats the bar (two bars) before it, shown as a repeat sign `%`; the notes are copied so playback is unchanged; editing the bar's notes makes it an ordinary bar again |
| `:ending 1 2` / `:ending none`                                       | the alternate endings the cursor bar belongs to (the same again clears them); `1-3` is a range                                                                                              |
| `:direction da-capo-al-coda`                                         | toggle a direction over the cursor bar: `fine`, `segno`, `double-segno`, `coda`, `double-coda`, `da-capo`, `da-capo-al-coda`, `dal-segno-al-fine`, `da-coda` … (Tab completes the list)   |
| `:double-dot`                                                        | toggle a double dot on the cursor beat                                                                                                                                                      |
| `:beams auto\|force\|break\|break-secondary` / `:stems auto\|up\|down` | beaming and stem direction of the cursor beat, or of every beat of the Visual selection (`force` joins a beat to the one before it, `break` starts a new group, `break-secondary` keeps the primary beam but breaks the shorter ones; `auto` clears the override) |
| `:accidental double-sharp\|double-flat`                              | the cursor note on a standard staff, spelled with a double sharp / flat (also `sharp`, `flat`, `natural`)                                                                                   |
| `:toggle {technique}`                                                | on the cursor note or beat: `accent` `heavy-accent` `staccato` `fermata` `let-ring` `palm-mute` `hammer` (legato) `bend` `half-bend` `vibrato` `vibrato-wide` `harmonic` `artificial-harmonic` `pinch-harmonic` `tremolo` `tap` `left-hand-tap` `slap` `pop` `slide` (shift) `slide-legato` `slide-in-above` `slide-in-below` `slide-out-down` `slide-out-up` `grace-before` `grace-on` `stroke-down` `stroke-up` `arpeggio-down` `arpeggio-up` `rasgueado` `pick-down` `pick-up` `crescendo` `decrescendo` `fade-in` `fade-out` `volume-swell` `tie` `dead` `ghost` |
| `:note C#4` / `:voice 2`                                             | set the cursor note's pitch / edit voice N (1–4) of the bar, creating it empty if new                                                                                                                                            |
| `:tuning drop-d` or `:tuning D2 A2 D3 G3 B3 E4` / `:capo 2`          | retune keeping pitches                                                                                                                                                                      |
| `:refinger` / `:transpose 2`                                         | re-finger the track to minimise hand movement / transpose                                                                                                                                   |
| `:track-add bass` / `:track-delete` / `:new [modern\|chant]`                         | tracks and new scores (`:track-add piano` gets a grand staff; refused on a chant score, which holds one chant track: `:chant-to-modern` first)                                                                                                                                 |
| `:staff-add [staff…]`                                               | add one or more staves at the bottom of the cursor's track, in the order given (`:staff-add treble tab slash`); staves only show the track's notes, so each track can have as many as you like |
| `:notation {notation}` | show the cursor's staff in another notation: `standard` (keeps the staff's clef), `tab`, `tab+` (tab with a rhythm row of note glyphs), `slash`; a clef name (`treble`, `bass`, `grand [split]`, `neutral`) is accepted too and sets it. A chant track has one square-notation staff, which this refuses to change; convert it with `:chant-to-modern` first. A tab on a track without strings puts its notes on strings (refused if one would not fit). `:staff` is the old name |
| `:clef {clef}` | the clef of the cursor's staff, by its notation. Standard: `treble`, `bass`, `grand [split]` (a grand staff splits at C3 unless a note such as `G3` is given) or `neutral` (percussion clef: drum tracks only, and the only clef they take). Tab and slash staves have no clef. A chant track takes `c1`–`c4` / `f1`–`f4` (the staff line, 1 is the bottom one) and `cb3` (flat clef); its notes keep their staff positions |
| `:lyrics-attach {above\|below} [track]` / `:lyrics-attach none` | attach lyrics to the cursor's staff, above or below it (not both), taken from the given track (number; the staff's own track by default) and aligned with the staff's beats by their position in the bar. A track whose staves have none attached shows its own lyrics below its top staff; once any staff has some, only those staves show lyrics |
| `:staff-remove [n]` / `:staff-move {n\|+N\|-N}`                      | remove a staff (1 = top; the cursor's by default; the last one stays) / move the cursor's staff to position n, or down / up |
| `:lyrics TEXT`                                                       | add lyrics from the cursor on: `(bar:beat)` (the cursor's position) and the words go at the end of the track's lyrics text, as if typed in the lyrics text area (`\n` is a line break). If the text already reaches past the cursor the words go on the next verse row ([LYRICS.md](LYRICS.md#6-commands)) |
| `:lyrics-clear [verse]`                                              | remove the track's lyrics, or the syllables (with their labels, dashes and gaps) on one verse row; later rows move up                                                                     |
| `:speed 75`                                                          | playback speed in percent                                                                                                                                                                   |

With the cursor on a note on any staff, the same note on the track's other staves is drawn in the current mode's color (GUI): the notehead, slash or square note on staves of lines, the fret number in bold on other tabs. A chord lights only the note under the cursor; nothing is lit on a rest or an empty position.

### Dashboard

With no buffer open (the last one closed with `:bd`, the only view closed with `:view-close`, or a
new, empty workspace) the window shows the dashboard: **Open Recent** (`:pick recent`), **Select
Workspace** (`:pick workspace`), **New Score** (`:new`) and **Open Score** (`:open`). Closing the
last *pane* does not show it: that is refused with a message.

Open Recent lists the files you closed lately (most recent first, up to 20, kept between sessions in
`recent-files` in cecilia's data folder, files that no longer exist skipped); choosing one opens it.
Without any the button is dimmed and does nothing, and the focus skips it.

`j` / `k` / `Down` / `Up` / `Tab` / `Shift-Tab` move the focus (`Home` / `End` first and last), `Enter` or
`Space` runs the focused button, a click runs the one under the mouse. Those plain keys come first;
every other key goes through the normal keymap, user bindings included, so quitting (`<C-q>` or
your own binding), `:`, `gt` / `gT`, `:view-new`, `:open`, `:new`, the pickers and `<F1>` all work.
What would edit the score hidden behind the dashboard (inserting, deleting, pasting, operators) does
nothing, and Insert and Visual mode are not entered. Showing a score ends the dashboard.

The dashboard has no tabs, so the tabline is not drawn over it (with several views it stays, for
the view tabs, without buffer tabs) and its space goes to the dashboard; the placeholder score
behind it is never listed as a tab or in the pickers. The dashboard belongs to a view: a view can
start on it (`ui.keep_files_in_new_view = false` makes `:view-new` do that), and workspaces save which
views show it.

### Workspaces

A workspace saves the views — each with its pane splits, name and bufferline of loaded files —
and each pane's file, cursor, recent files, layout and waveform strip, each pane's zoom, follow, chant
view, the window waveform bar, marks (of saved files) and recorded macros, to
`~/.local/share/cecilia/workspaces/NAME.toml` (or a path). A workspace is saved when you ask
(`:workspace-save`), when you switch to another, and when cecilia quits (`ui.workspace_autosave_on_quit`,
on by default; `:quit!` never saves it). There can also be *no* workspace: the status line shows
none, and quitting saves nothing.

`cecilia` with no arguments loads the workspace used last, or creates `default` if none is saved
(`ui.workspace_restore_last = false`: the dashboard, with no workspace). `cecilia --workspace NAME`
starts with another (created from scratch if it doesn't exist yet). `cecilia FILE…` opens just
those files, with no workspace, so one-off edits leave your workspaces alone — `:workspace-save
NAME` keeps such a session if you want it (with `--workspace` too, the files open in that
workspace).

| command                   |                                                                                                                                  |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `:workspace-open [name]`  | save the current workspace, then switch to `name` (created, empty, if it doesn't exist); no name: the picker                       |
| `:workspace-open! [name]` | the same without saving the current workspace first, and dropping unsaved changes in files (which stop `:workspace-open`)         |
| `:workspace-save [name]`  | save the session as workspace `name` and make it the active one, leaving the old file as it was (a copy); no name: the active file |
| `:workspace-save! [name]` | the same, overwriting an existing workspace of that name (without `!` that is refused)                                           |
| `:workspace-delete [name]` | delete that workspace file after asking (`y`/`n`); no name: the active one, leaving no workspace (the session stays open)         |
| `:workspace-delete! [name]` | the same without asking. Deleting `default` is allowed: it is created again when cecilia next starts with no arguments            |

### Swap files

While a file has unsaved changes cecilia keeps a recovery copy in the swap directory (`[swap]` in
the config: `enabled`, `interval`, `auto_recover`, `dir`; by default `$XDG_STATE_HOME/cecilia/swap`, else
`~/.local/state/cecilia/swap`), named after the file's full path. It is deleted when the file is
saved or closed, and on quit. Opening a file whose swap file is newer (and not owned by another running cecilia) restores it
automatically as unsaved changes (`auto_recover`, on by default; the file on disk is untouched
until you `:w`; the undo tree is kept, and `:bd!` then `:open` gets the saved file back). With
`auto_recover = false`, or when another running cecilia owns the swap file, it says so and stops
autosaving that file until you choose:

| command                  |                                                                                  |
| ------------------------ | -------------------------------------------------------------------------------- |
| `:recover`               | restore the file from its swap file, left unsaved (`:recover!` if another cecilia owns it) |
| `:recover swapfile`      | open a swap file as a new unnamed buffer (for scores that were never saved)      |
| `:swap-discard`          | delete the file's swap file                                                      |

### Chant _(M6)_

Which commands apply depends on the kind of score. Every command is for **both** kinds, for
**modern** scores or for **chant** scores. In a chant score `:` completion, the command and key
lists (`<F1>`, the keymaps picker) and the which-key popup show the both and chant commands; in a
modern score the both and modern ones. A command of the other kind still parses when typed in full
and answers with the kind (`capo: not in a chant score`, `chant-mora: not in a modern score`)
instead of "unknown command". Chant-only commands are the `:chant-*` ones, `:gabc`,
`:interpretation` and `:chant-to-modern`; modern-only are durations, tuplets, rests, ties,
effects, dynamics, tuning, capo, staves, `:lyrics`, bar insertion, `:modern-to-chant` and the like.
The chant score has its own normal-mode layer, `[keymap.normal-chant]` (`[keymap.chant]` is
accepted as well), looked up before `[keymap.normal]` and only on chant scores:

| keys | command |
| --- | --- |
| `tp` `ti` `tv` `tV` `ts` `tq` `to` `tu` | head: punctum, inclinatum, virga, bivirga, stropha, quilisma, oriscus, quadratum (`:chant-shape`) |
| `t~` `t<` `t>` | liquescence: deminutus, auctus ascendens, auctus descendens |
| `tm` (`t.`) `te` `tt` | mora, horizontal episema, ictus |
| `tb` `tn` | flat, natural before the note |
| `t*` `t+` | `*` or `+` in the syllable's text |
| `td,` `td;` `td:` `tD` | division `,` `;` `:` `::` after the syllable |
| `tc1`–`tc4` `tcb` `tF1`–`tF4` | clef do 1–4, flat do, fa 1–4 |
| `<leader>yh` `<leader>yu` | audio sync: set the cursor note at the playhead (`:sync-here`), take back the last tap (`:sync-undo`) |
| `<leader>y[` `<leader>y]` `<leader>y{` `<leader>y}` | nudge the sync point under the cursor by −10 / +10 / −50 / +50 ms (`:sync-nudge`) |
| `<leader>yy` `<leader>yn` | tap per syllable / per note (`:sync-unit`) |

**Sync mode.** `S` (or `:sync`) enters Sync mode (status line `SYNC`; `:sync` or `<Esc>` leaves it) and opens
the waveform pane if it is not open. The waveform shows the sync points (a yellow line each) and its
rulers only in this mode. Keys (`[keymap.sync]`, with Normal mode's motions and anything that changes
nothing): `<Space>` plays and pauses (`:play-toggle`); `<CR>` syncs the note under the cursor at the
playhead (a chant: moved to the strongest onset within `ui.sync_snap` ms, 70 by default, 0: off) and moves the cursor on by the sync unit
(`:sync-tap`); `u` / `<BS>` takes the last tap back; `x` (`:sync-remove`) removes the sync point at the cursor (or of the selection); starting playback always starts at the cursor; `[` `]` `{` `}` nudge the point under the cursor
by −10 / +10 / −50 / +50 ms; `y` / `n` set the unit (a syllable's first note / every note). Enter passes over rests and divisions (syllables
without notes). `:` opens the command line
and returns to Sync mode. The sync of a chant is kept on notes, so editing the chant does not move it.
`:sync-detect` on a chant finds the notes sung in the recording by pitch and lays the chant's notes
along them in order (allowing for a transposition and for notes left out), placing a point at each
syllable (each note with `:sync-unit note`); fix the rest by hand.

**Syncing a modern score** works the same way, by beat: every beat (rests included) of the cursor's voice
takes a sync point of its own, kept on the beat's stable id so that editing the score does not move it
(it is saved in `.cc` files; Guitar Pro files keep the derived bar points). `<CR>` syncs the beat under
the cursor at the playhead and moves to the next beat; `x`, `u`, `[` `]` `{` `}`, `d` / `c` over a range, Visual mode and the
mouse on the waveform do what they do on a chant. The waveform's top ruler has a tall tick at the first
beat of every bar (`bar:1`) and a short one at the others (the beat). There is no sync unit (a beat is
the unit) and `:sync-detect` is still beat detection. A bar-synced score (a Guitar Pro file) keeps its
points when the first beat is synced. Repeated bars are synced on their first play.

In Sync mode `d{motion}`, `dd` and `c{motion}` take away the sync points of the range (nothing else is
changed, and `c` does not start Insert mode); in Visual mode (`v` from Sync mode) `d` / `c` do so for the
selection and `[` `]` `{` `}` move every sync point in it. On the waveform of a chant, a mouse press adds a sync
point on the syllable (or note, by the sync unit) nearest the mouse, the cursor following (snapped to an onset, `ui.sync_snap`) unless one is within 5 pixels, which
is grabbed; the audio plays from there as long as the button is held, and dragging moves the point
(one undo step). Outside Sync mode a drag scrolls the waveform. `:zoom-in`, `:zoom-out` and `:zoom-reset`
zoom the pane that has the focus: the waveform pane's timeline when it does.

On a chant track the cursor moves note by note (`w` `e` `b` go to the next note, the end of the
note, the previous note; the cursor stands on the staff line of the note, `j` / `k` move it to
choose the pitch of a note, and in Insert mode `<Space>`, `<CR>` and `/` enter the note on that
line); `<C-a>`/`<C-x>` move a note up or down a staff
position, `:accidental flat` adds a flat, `x` deletes it and `u` undoes (see "Insert mode" above for the
chant keys, and which modern-track keys answer "not in chant").

| command                                      |                                                                                                       |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `:gabc` / `:gabc fgh` / `:gabc Glo(fg)ri(h)` | show the cursor syllable as GABC; replace its notes, or the syllable itself                           |
| `:interpretation [equalist\|solesmes]`       | rhythm used for playback and the modern transcription                                                 |
| `:clef c3`                                   | change the chant clef (`c1`–`c4`, `f1`–`f4`, `cb3` for a flat clef); notes keep their staff positions |
| `:chant-shape NAME`                          | the cursor note's head: `punctum`, `inclinatum`, `virga`, `virga-reversa`, `bivirga`, `trivirga`, `stropha`, `bistropha`, `tristropha`, `quilisma`, `oriscus`, `oriscus-stem`, `quadratum` (again takes it off) |
| `:chant-division TOKEN`                      | a division after the cursor syllable: `,` `;` `:` `::`, `;1`…`;6` (the same again takes it off, another replaces it) |
| `:chant-clef NAME`                           | the chant's first clef (`:clef` does the same)                                                        |
| `:chant-mora` `:chant-episema` `:chant-ictus` | toggle the mora, the horizontal episema or the ictus of the cursor note                              |
| `:chant-alter flat\|natural`                 | the flat or natural before the cursor note                                                            |
| `:chant-liquescence deminutus\|ascendens\|descendens` | toggle a liquescence of the cursor note                                                     |
| `:chant-symbol SYMBOL`                       | add `*` `+` `†` `℣` `℟` to (or take it off) the cursor syllable's text                                |
| `:score-kind`                                | say whether this is a modern or a chant score and which command converts it (what clicking `MODERN` / `CHANT` in the status line does) |
| `:modern-to-chant`                           | open the cursor track's melody as chant in a new buffer `[chant] name` (syllables from lyrics; a chord keeps the note `ui.chant_chords` picks); other tracks are left out |
| `:chant-to-modern`                           | open the chant score as a modern score in a new buffer `[modern] name` (bars as a pitched track, syllables as lyrics); the chant buffer is unchanged |

### Play-along _(M5)_

| command                                    |                                                                                                                                                                                                          |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `:audio [file]` / `:audio off`             | attach a recording (mp3, m4a/aac, flac, ogg, wav; relative to the score). Without a file, the best name match next to the score or in its `mp3/`/`audio/` folder                                         |
| `:sync`                                    | enter (or leave) Sync mode: the waveform shows the sync points and rulers, `<Space>` plays, `<CR>` syncs the cursor's cell (a modern score: the bar) and moves on, `<Esc>` leaves                                                                                                                |
| `:sync-tap` / `:sync-clear`                | sync the cursor beat (chant: note) at the playhead and move on / remove all sync points                                                                                                                                             |
| `:sync-unit syllable\|note`                | chant: what one sync tap sets (default a syllable; also a row in the chant Score tab)                    |
| `:sync-remove`                             | chant: remove the sync point of the cursor (or selection)                                               |
| `:sync-undo`                               | chant: take back the last sync tap (while tapping)                                                      |
| `:sync-here` / `:sync-nudge MS`            | sync the cursor beat (chant: cell) at the playhead / move the sync point under the cursor by MS milliseconds |
| `:sync-detect`                             | modern: beat-detection assist: fill in sync points from the last existing one (tap the first downbeat first for best results); chant: place the notes along the pitches sung                                                                                    |
| `:mix [both\|audio\|midi]`                 | what plays when a recording is attached (no argument cycles)                                                                                                                                             |
| `:volume midi\|audio\|metronome\|master {percent}` | source levels (`master` scales the whole output)                                                                                                                                                                                            |
| `:mute` / `:solo [n\|audio\|master]`       | toggle a track's (cursor track by default), the audio track's or the master's mute / solo                                                                                                                                                |
| `:loop [a b]` / `:loop-toggle`             | loop bars a–b (1-based), or the selection / cursor bar                                                                                                                                                   |
| `:trainer start target step`               | speed trainer: start at `start`%, add `step`% each loop up to `target`% (e.g. `:trainer 60 100 5`)                                                                                                       |
| `:pitch {semitones}`                       | shift the recording's pitch (−12…12, keeps tempo)                                                                                                                                                        |
| `:metronome` / `:count-in` / `:waveform-strip` | toggles (`:waveform-strip`: the focused pane's strip) |
| `:waveform [on\|off]`                      | the waveform tool pane: the score pane's audio waveform, marking the play position (or the cursor when stopped); no argument toggles |
| `:export {file}`                           | export by extension                                                                                                                                                                                      |
| `:design [on\|off\|toggle]`               | design mode (page layout only): the layout can be adjusted by hand, with handles in the GUI and the keys of `[keymap.design]`; the status line shows `design`. The adjustments are saved in `.cc` files |
| `:bar-width N\|+N\|-N\|auto`              | width of the cursor bar in the page layout, in percent of its automatic share of the line (the other bars of the line give up or take the difference so the line still fills the page; a bar is never narrower than 35% of its natural width) |
| `:bars-per-line N\|+N\|-N\|auto`          | exactly N bars on the cursor's line (squeezed if need be) from the line's first bar on; `auto` hands the line back to the layout |
| `:line-break` / `:line-join`              | start a new line at the cursor bar / bring the cursor bar back onto the previous line |
| `:beat-nudge N\|+N\|-N\|reset`            | move the cursor beat sideways (tenths of a staff space; notes, tab, lyrics and marks on the beat move together) |
| `:design-reset`                           | discard every manual adjustment of the score |
| `:layout [page\|vertical\|horizontal]`     | GUI score layout of the focused pane _(M3)_                                                                                                                                                              |
