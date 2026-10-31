# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The edit cursor now shows on empty bars, e.g. after inserting a bar with `o`.

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
