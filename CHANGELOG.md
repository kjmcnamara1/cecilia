# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.17.0] - 2026-11-04

### Added

- Design mode for the page layout (`:design`, `<leader>d`): fix clashes by hand. Bar widths (`:bar-width`), bars per line (`:bars-per-line`, `:line-break`, `:line-join`) and beat positions (`:beat-nudge`) can be set with commands and the keys of the new `[keymap.design]`, or in the GUI by dragging bar lines and the dots under the cursor bar's beats (live re-flow) and clicking the "+" / "-" buttons at the end of each line to add or remove a bar on it. Every change is an undo step (one drag is one); `:design-reset` returns to the automatic layout. The adjustments are stored in `.muse` files and follow bars through inserting, deleting and pasting.

## [0.16.0] - 2026-11-04

### Fixed

- On a slash staff the cursor always sits on the staff's single line: clicking above or below the line selects the beat only, and the cursor's line is normalised whenever it lands on a slash staff (click, `<A-j>` / `<A-k>`, `j` / `k`, loading a score).

### Changed

- On a chant staff the status line shows the solfège syllable of the cursor's position (`do` `re` `mi` `fa` `sol` `la` `ti`, read from the clef in force there) instead of the GABC letter.

### Added

- The status line shows, in every mode, how full the cursor's bar is, as quarter-note beats of the voice under the cursor over the time signature's length (`3.0/4.0`, `4.5/4.0`; 6/8 is `3.0`, 7/8 `3.5`), in the error color unless the bar is complete. Free-time bars and a pickup bar show nothing.
- `ui.highlight_incomplete_bars = true` (and `:incomplete-bars on|off|toggle`) draws the staff lines of bars that are too short or too long in red, on every kind of staff in the GUI and on the dashed staff lines of the terminal tab view. The color is the new theme color `incomplete_bar`.

## [0.15.0] - 2026-11-03

### Added

- Vim-style editing workflow. Normal mode: `w` / `e` / `b` move along the cursor's string (tab) or line / space (standard) by notes and ties (`:note-row-next`, `:note-row-end`, `:note-row-prev`); `^` / `gh` and `$` / `gl` go to the first / last beat of the bar with a note (`:bar-first-note`, `:bar-last-note`); `I` / `A` insert a beat at the start / end of the bar and start typing (`:insert-bar-start`, `:append-bar-end`); `dl` `dh` `cl` `ch` `yl` `yh` act on the beat under / before the cursor, also on the last beat of the score.
- Visual mode: `i` / `a` insert a beat before / after the selection (a bar selection: at the start of its first bar / the end of its last) and start typing (`:visual-insert`, `:visual-append`).
- Insert mode: Enter finishes the beat (on a standard staff it puts a note at the cursor's line or space, on a slash staff a slash; a tab's fret is already in) and moves to the next beat, to the start of the next bar after the last complete beat, with the duration chosen in the palette (`:insert-enter`); `h` `l` `j` `k` move the cursor (not on chant, where they are pitches); `<A-CR>` adds a beat without typing into it.
- Normal-mode keymaps per notation, `[keymap.normal-tab]`, `[keymap.normal-standard]`, `[keymap.normal-slash]` and `[keymap.normal-chant]`, looked up before `[keymap.normal]` like the insert ones; on a standard staff `t#` `t&` `t=` make a note sharp, flat or natural.
- The editing palette lists only the tools that apply to the mode and the staff under the cursor: accidentals on standard staves, bends and palm mute on tabs, note values and ties on slash, note values only on chant and in Visual mode.
- Palette tools: double dot, fermata, double sharp / flat, crescendo / decrescendo, fade in / out and volume swell, artificial harmonic, left-hand tapping, grace notes before / on the beat, the six slides (shift, legato, in from above / below, out downwards / upwards), arpeggio down / up, rasgueado, pickstroke down / up, beams (automatic, force, break, break secondary) and stems (automatic, up, down), and the bar tools: clef, key and time signature, free time, double bar line, repeat one / two bars, repeat open / close, alternate endings, directions and section. Tools that need an argument open the command line with the command typed. Commands: `:double-dot`, `:free-time`, `:double-bar`, `:bar-repeat 1|2|off`, `:ending`, `:direction`, `:beams`, `:stems`, `:accidental double-sharp|double-flat`, more `:toggle` techniques; `:key` also takes names (`Bb`, `F#m`).
- The palette searches like the properties pane: `/` opens a field, the selection jumps to the first matching tool or section as you type (name, tooltip, command or section title), `<Esc>` goes back, `<CR>` keeps the jump, `n` / `N` repeat (`:palette-search`, `:palette-search-next`, `:palette-search-prev`); only the tools shown for the staff and mode are searched.
- Palette icons: the GUI draws the engraver's own music-font glyphs (notes and rests, accidentals, dynamics, fermata, repeat signs, clefs, bow signs …) and pictograms for beams and stems; the terminal uses the closest Unicode / Nerd Font music glyphs, following `tabline.icons` (`none`: short text labels).
- New score data, kept in `.muse` files (older files still open): bar repeat signs, per-beat beam and stem overrides, fermatas, fade out and volume swell, left-hand taps, and the written double sharp / flat of a note. The engraving honours the beam and stem overrides and shows fermatas, pick strokes, fades, repeat signs and double accidentals; directions print as "D.C. al Coda" and the like. Guitar Pro 7 files keep fades, left-hand taps and repeat signs; MusicXML keeps stems, fermatas and double accidentals.

### Changed

- `i` and `a` now add a rest beat before / after the cursor beat (in the insert duration) and insert into it; `o` and `O` add a bar and start Insert mode. The old `i` (Insert mode on the cursor beat, adding nothing) is `gi` / `:insert-here`; `:bar-insert-after` / `:bar-insert-before` still add a bar without starting Insert mode.
- Normal `w` / `b` follow the cursor's row and ties; the old next / previous beat with a note is `W` / `B`. `$` is the last beat with a note (the last beat of the bar is `<End>`, the first `0` / `<Home>`).
- `x` deletes only the note under the cursor, also on a piano or other non-fretted track, where it used to clear the beat (on a slash staff it still clears the beat).
- `c` over beats (`cl`, `ch`, `cw`, visual `c` …) turns them into rests of the same lengths instead of deleting them and adding one beat; `cc` clears only the cursor track's bar.
- Insert mode: `<CR>` is Enter as above (a new beat after the cursor is `<A-CR>`); on a tab `h` and `l` (hammer-on / pull-off, let ring) are `H` and `L`, and on a standard staff hammer-on / pull-off is `H`.

## [0.14.0] - 2026-11-03

### Added

- `:clef` picks the clef of the cursor's staff by its notation: `treble`, `bass`, `grand [split]` or `neutral` (the percussion clef, now the clef of drum staves) for standard notation, `do` or `fa` for chant (a chant staff on an ordinary track fits its melody to a C or an F clef). Tab and slash staves have no clef. On a chant track it still takes `c1`–`c4`, `f1`–`f4` and `cb3`.
- `:lyrics-attach above|below [TRACK]` (or `none`) attaches lyrics to the cursor's staff, above or below it, taken from any track (the staff's own by default) and aligned with the staff's beats by position in the bar. Lyrics above a staff sit just over the highest notation, stem, mark or chord name over them, and under a section marker. Lyrics below a staff (the track's own or attached) are placed under whatever hangs below it, including dynamics and hairpins, which now sit closest to the staff; syllables that fall on the same place or too close are set one after another, never overlapping.
- `ui.bar_numbers = "every"` (and `:bar-numbers every|system|toggle`) numbers every bar in the engraved score, not just the first bar of each line.

### Changed

- A grand staff now splits between its treble and bass halves at C3 (MIDI 48) by default instead of middle C; `:clef grand NOTE` still sets another split.
- `:staff` is now `:notation NOTATION` (`standard`, `tab`, `tab+`, `slash`, `chant`); `:staff` still works as an alias. `:notation standard` keeps the staff's clef. `<A-j>` / `<A-k>` still move between staves.

## [0.13.1] - 2026-11-03

### Fixed

- GUI: scrolling to keep the edit cursor in view now targets the cursor itself (its string or staff position) instead of the whole beat's height, so a zoomed view no longer snaps to the top staff; moving between staves or strings in the same beat also scrolls. The cursor is always scrolled fully into view, horizontally too, at any zoom (including a bar wider than the pane and the centred page layout).

## [0.13.0] - 2026-11-03

### Added

- `<F1>` shows the key bindings of the current mode in every mode (insert, visual, command line, picker and tool panes included), from the effective keymaps; `<F1>` or `<Esc>` hides it.

## [0.12.0] - 2026-11-03

### Added

- The GUI edit cursor has a rounded outline (theme color `cursor_border`, default contrasting with the page) so it shows in every mode.

## [0.11.1] - 2026-11-03

### Changed

- The `tab+` staff engraves rests in the middle of the strings, and both tab staves set their time signature in standard-size digits close together on the middle line.

## [0.11.0] - 2026-11-03

### Added

- `ui.chant_chords` (`top` default, `bottom`, `error`): which note of a chord the chant staff of
  an ordinary track shows, since chant is monophonic. With `error` no chant staff is drawn: it
  shows, and the status line says, where the first chord is ("Chant staff: chord at bar 12 beat 3
  — chant is monophonic"). `:modern-to-chant` follows the same setting.
- The chant staff of an ordinary track shows the neumes `:modern-to-chant` produces: notes are
  grouped into syllables (lyrics, or quarter notes) and neumes (pes, clivis, torculus, porrectus,
  climacus, scandicus …) by shared code in `muse-core`, joined by slanted strokes, with the same
  clef and transposition, mora on long notes and divisions after rests (a rest of a quarter or
  more is `:`, a shorter one `,`), double bars (`:`) and the end (`::`). A beat's own chant
  division wins over those. `neume_start` marks still force a new neume.
- A test checks that playback is the same whichever staves a track shows.

### Changed

- Square notation (GABC files and chant staves of ordinary tracks) is set with the bundled Caeciliae
  chant font (SIL OFL 1.1): its heads, podatus, clivis, porrectus, virga, inclinata, quilisma,
  oriscus, stropha, clefs, custos, divisions, flats and mora. Neumes are composed from its
  glyphs and connecting lines; a neume is set whole at its first note, so it stays joined
  across a barline or line break.
- The chant staff's four lines now span the height of a modern five-line staff (ordinary tracks
  and GABC); note heads are taller. A pes is a stacked column, a clivis has its stem, a
  torculus sits side by side, and falling runs of three or more notes are a climacus (a virga
  then diamonds, also in `:modern-to-chant` output). The custos sits on the open end of the
  staff lines.
- Square notation: note heads with flat sides and an arched top and bottom (the lower note of a
  pes nearly square), larger mora dots, a tall rounded C clef with a round notch, a custos and a
  red initial; the same in GABC files.
- On the chant staff of an ordinary track only the first note of a neume sits in the beat column
  shared with the other staves; the rest are packed right after it and joined, across barlines
  too. The staff has no barlines, only divisions, and a custos at the end of each line shows the
  next line's first note.
- The edit cursor on a chant staff is a single square on the note position of that staff, not a
  column across all staves; `<A-j>` / `<A-k>` onto a chant staff land on the beat's note.

## [0.10.1] - 2026-10-31

### Fixed

- `<C-d>` / `<C-u>` (and `<PageDown>` / `<PageUp>`) move the cursor half a screen: half the
  displayed bars in the horizontal layout; in the vertical layout half the displayed rows of
  bars (rounded up, at least one), and in the page layout half the rows on a page whatever the
  zoom, staying at the same place in the bar and on the same line.

## [0.10.0] - 2026-10-31

### Added

- `:staff-next` / `:staff-prev` (`<A-j>` / `<A-k>`) move the cursor between the staves of a track.
- `<A-j>` / `<A-k>` select the next / previous item in pickers and in command-line completion,
  like `<Tab>` / `<S-Tab>`.
- `:focus-next` / `:focus-prev` (`<C-w>w`, `<C-w>W`) cycle the focus through the panes of the view.
- `:focus-last` (`<C-w>p`) switches back to the previously focused pane.

### Changed

- `j` / `k` stay on the cursor's staff, with four ledger-line positions above and below a staff
  for extra notes, instead of running on into the next staff. Set `ui.jk_moves_between_staves =
  true` for the old behaviour.

## [0.9.0] - 2026-10-31

### Changed

- Swap files keep the undo tree, so recovering unsaved changes (automatic or `:recover`) no longer
  loses undo history. Version 1 swap files still load, without history.
- Closing a buffer or view, or quitting, with unsaved changes asks whether to save each changed
  file (`y`), discard it (`n`) or cancel, instead of refusing.
- A new score is not marked as changed until it is edited, so an empty one closes without a prompt.

### Fixed

- Directional pane focus (`focus-left` etc.) goes to the most recently focused of the panes
  touching the current one, instead of the one nearest its centre.

## [0.8.0] - 2026-10-31

### Added

- Shift and Super (Windows/Cmd) in key notation: `<C-S-q>` is distinct from `<C-q>`, `<S-F5>`,
  `<S-Up>`, `<D-q>` (aliases `Super-`, `Win-`, `Cmd-`). Shift on a plain character still folds
  into it (`Q` is `<S-q>`). The GUI reports both modifiers; the terminal UI enables the kitty
  keyboard protocol where supported.

## [0.7.0] - 2026-10-31

### Added
- Swap files: with `[swap] auto_recover` (default on) opening a file with a newer swap file restores it as unsaved changes. buffers with unsaved changes are autosaved to a recovery copy in the swap directory (`[swap]` `enabled`, `interval`, `dir`; default `$XDG_STATE_HOME/muse/swap`, else `~/.local/state/muse/swap`), removed on save, close and quit. Opening a file with a newer swap file offers `:recover` (`:recover!`, `:recover swapfile`) and `:swap-discard`.

## [0.6.1] - 2026-10-31

### Fixed

- The edit cursor now shows on empty bars, e.g. after inserting a bar with `o`.

## [0.6.0] - 2026-10-31

### Added

- Editing palette along the left of the window, above the bottom tool panes, which keep the full width, also in restored workspaces (`<leader>e` shows it, `<leader>E` focuses it, `ui.palette` (off by default), `:palette`, `[keymap.palette]`): a grid of icon buttons in collapsible sections (durations, dotted/triplet, rest, accidentals, ties, dynamics, articulations, effects) that apply to the score pane worked on, like Guitar Pro's editing panel. Buttons in force at the cursor are tinted and follow it; one gray highlight is shared by the mouse and the keyboard selection, and its button's name, with its insert-mode keys, shows at the bottom of the pane (and as a tooltip on hover; the terminal draws short labels). `h`/`j`/`k`/`l` move through the grid, `<CR>` applies, `za` folds. Buttons that can't take effect at the cursor (a note-level edit on a rest, an accidental on a tab) are dimmed, and applying one only says why. New `:dynamic ppp|pp|p|mp|mf|f|ff|fff` sets the note's velocity.
- `:staff-add` takes several staves, added in the order given (`:staff-add treble tab grand G3`).

## [0.5.0] - 2026-10-31

### Added
- With the cursor on a note on any staff, the same note on the track's other staves is drawn in the current mode's color: the notehead, slash or square note on treble, bass, grand, slash and chant staves, and the fret number (in bold) on other tab staves.

### Fixed

- Opening a workspace with a pane kind this build doesn't know drops that pane instead of showing it as a score pane.
- The progress bar no longer panics in a pane narrower than a pixel.

## [0.4.0] - 2026-10-30

### Added
- Toolbar buttons for the properties pane, waveform, piano, fretboard and tuner, in the group with the tracks button (they show whether their pane is on).
- Properties pane along the right of the window (`<leader>p` shows it, `<leader>P` focuses it, `ui.properties` (off by default), `:properties`) with three tabs: SCORE (metadata, page setup), TRACK (name, kind, color, tuning, frets, capo, staves, instrument, channel, bank, volume, pan, mute, solo) and LYRICS (a text area in Guitar Pro lyric syntax, laid onto the notes). Each property is a label over a full-width value box, in sections that fold (`za`, `zc`, `zo`). Editing a value is insert mode, applied to the score as you type as one undo step (`<Esc>` cancels; the lyrics text area is edited like vim, with its modes in the status line). Color opens a picker of named colors with swatches and hex codes (type `#rrggbb` or `rgb(r,g,b)` for a custom one); tuning opens a picker of common tunings for the track's instrument (type notes for a custom one). Edits are undoable. Staves are rows (a picker of staff kinds, an empty row to add one, drag or `<A-j>`/`<A-k>` to reorder), mute and solo are switches, `H`/`L` change tab, `c` clears and edits, `d` clears. `h`/`l` set switches and step through choices (no wrapping), icons show staves and instruments, only a double click edits, and a line shows where a dragged staff lands. Standard staves are named `treble`, `bass` and `grand` (`:staff grand G3`); `standard …` still parses. The wheel scrolls the pane and pickers; numbers step with `h`/`l`; arrows and `<Tab>` follow `hjkl`. `/` searches the pane (and the lyrics text) with `n`/`N`. Slash staves draw a single line with their stems always up. A Guitar Pro file's lyric line breaks show in the pane. The wheel scrolls the lyrics text, and a click there places the caret, a drag selects, a double click selects a word (or enters insert mode from normal mode), `<C-a>` selects all, and `<C-c>`/`<C-x>`/`<C-v>`, `y`, `yy`, `p` and `P` use the system clipboard.
- Scores keep their page setup, read and written by Guitar Pro 5 files.

### Fixed

- `j`/`k` stop at the last/first row of a track instead of jumping into the neighbouring track.


## [0.3.0] - 2026-10-30

### Added

- Scrollbars on score panes (`:scrollbar`, `ui.scrollbar`): pill-shaped thumbs in a light tint of
  the background, in their own gutter so they never overlap the notation. Click or drag to
  scroll, in the GUI and TUI.
- GUI: score panes scroll both ways when the score is bigger than the pane (a narrow vertical
  layout, a tall horizontal layout, a zoomed page), with a scrollbar for each direction. The
  vertical wheel scrolls down and the horizontal wheel (or Shift) across, in every layout.

### Fixed

- GUI: the vertical layout fits panes narrower than 300pt instead of cutting off the bars.

## [0.2.0] - 2026-10-30

### Added

- Waveform tool: `:waveform` toggles the pane and `:global-tool waveform` toggles its global status.
- Tracks pane refits its height when the buffer of a pane changes.

### Changed

- `close-pane` is now `pane-close`; `quit` (`:q`, `:qa`) exits muse instead of closing a pane; `write-quit` (`:wq`, `:x`) writes every changed score and quits.
- The audio tool is now called the waveform tool.
- Progress bar matches the Go Playalong look: paper background, solid black dividers, no bluish tint.
- The pane _maximized_ indicator in the titlebar is an icon instead of "Z".

### Fixed

- Progress bar's red position indicator is half as wide in the GUI.
