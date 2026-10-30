# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Properties pane along the right of the window (`<leader>p` shows it, `<leader>P` focuses it, `ui.properties`, `:properties`) with three tabs: SCORE (metadata, page setup), TRACK (name, kind, colour, tuning, frets, capo, staves, instrument, channel, bank, volume, pan, mute, solo) and LYRICS (a text area in Guitar Pro lyric syntax, laid onto the notes). Each property is a label over a full-width value box, in sections that fold (`za`, `zc`, `zo`). Editing a value is insert mode, applied to the score as you type as one undo step (`<Esc>` cancels; the lyrics text area is edited like vim, with its modes in the status line). Colour opens a picker of named colours with swatches and hex codes (type `#rrggbb` or `rgb(r,g,b)` for a custom one); tuning opens a picker of common tunings for the track's instrument (type notes for a custom one). Edits are undoable.
- Scores keep their page setup, read and written by Guitar Pro 5 files.

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
