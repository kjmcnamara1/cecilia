# cecilia

A keyboard-first, open-source editor and player for guitar tablature, standard notation and
Gregorian chant, with Guitar Pro compatibility and audio play-along. Written in Rust.

**Status:** milestones 1–7 (terminal reader, MIDI playback, file formats, engraving and GUI,
editing, audio play-along, Gregorian chant, drums, piano grand staff and lyrics). See [docs/SPEC.md](docs/SPEC.md) for the
full specification and roadmap, and [docs/keymap.md](docs/keymap.md) for key bindings.

## What works now

- Reading Guitar Pro 3–8 (`.gp3`, `.gp4`, `.gp5`, `.gpx`, `.gp`), MusicXML (`.musicxml`, `.mxl`),
  MIDI, GABC (Gregorio chant) and cecilia's own `.cc`; writing `.gp5`, `.gpx`, `.gp`, `.musicxml`,
  `.mxl`, `.mid`, `.gabc`, `.cc`
- Drums in standard drum notation (hands stems up, feet stems down, x heads for cymbals) and as
  drum tab in the terminal (CY/HH/T1/SN/FT/BD lanes), entered with letter keys in Insert mode
- Piano and other multi-staff parts as braced grand staves, kept as one instrument in GP7/GPX/
  MusicXML
- Lyrics: one gabc+ text per track (GABC with labels, italics, anchors), sung in playing order
  through repeats and D.C./D.S., several verses under the staff with centred hyphens, in the
  terminal under the tab, edited in the properties pane or with `:lyrics`; Guitar Pro lyrics of
  all versions are read (see [docs/LYRICS.md](docs/LYRICS.md))
- Gregorian chant: square notation (neumes, clefs, divisions, custodes, episemata, morae, drop-cap
  initial) in the GUI and PDF/SVG/PNG, a text staff in the terminal UI, playback, editing (`:gabc`,
  pitch keys, delete, undo), equalist or Solesmes rhythm, and conversion to and from modern
  notation (`:modern-to-chant` uses lyrics as syllables)
- GUI (wgpu/vello): engraved standard notation and tablature in split panes and tabs, with page,
  continuous-vertical and continuous-horizontal layouts, zoom, click-to-place cursor and a
  playback cursor; everything keyboard-driven with the same keymaps as the terminal UI
- PDF, SVG and PNG export of engraved notation
- Terminal UI: tablature view, vim-style navigation with counts, `:` commands, split panes
  (`<C-w>s` / `<C-w>v`), a bufferline of open files per view (`H`/`L`, `:view-new`, `gt`), which-key hints, mouse clicks and scrolling
- MIDI playback with a moving cursor, repeats and tempo changes, through a SoundFont or a built-in
  plucked-string synth
- Editing in both UIs: vim-style Insert/Visual modes and operators, an undo tree, macros, the
  Guitar Pro technique set, auto-fingering and retuning
- Audio play-along: attach an mp3/m4a/flac/ogg/wav recording (or use the one embedded in a GP7/8
  file), sync it by tapping bars or with beat detection, then play it alone or mixed with MIDI at
  any speed or pitch, with A–B loops, a speed trainer, metronome and count-in; a waveform strip
  shows the recording, sync points and loop in the GUI
- WAV/FLAC export of the MIDI rendering mixed with the recording
- A bufferline.nvim-style tabline: each view's open files as tabs (file-type icon, name
  shortened in the middle, hover close button) and the views on the right
- A tracks panel (`<leader>T`) as in Guitar Pro: per-track volume, solo, mute, instrument and a
  colored bar overview with the edit cursor and playhead
- A toolbar of common commands (configurable buttons), and themes in `[theme]`: six built-in
  presets, theme files, and every interface color (down to the score's page and ink)
- Workspaces: `:workspace-save NAME` / `:workspace-open NAME` / `cecilia --workspace NAME` keep the
  views, their pane splits and open files, saved when you switch and on quit; `cecilia` reopens the
  last one (or `default`), while `cecilia FILE` edits files without touching any workspace
- TOML configuration with custom keybinds, reloaded automatically when the file changes

## Usage

```sh
cargo run --release -- song.gp5            # open in the terminal UI
cargo run --release -- --gui song.gp5      # open in a window
cargo run --release -- info song.gp5       # tracks, tunings, tempo, length (--json for the model)
cargo run --release -- play song.gp5       # headless playback
cargo run --release -- convert in.gpx out.musicxml   # convert by output extension
cargo run --release -- convert song.gp5 song.pdf --track 2 --layout page   # engrave
cargo run --release -- convert song.gp song.flac     # render MIDI + recording to audio
cargo run --release -- convert hymn.gp5 hymn.gabc    # melody and lyrics as chant (lossy)
cargo run --release -- convert chant.gabc chant.pdf  # square notation
```

In the UI: `h j k l` move by beat/string, `w b` jump between notes, `{ }` bars, `J K` tracks,
`<Space><Space>` play/pause, `<Space>s` stop, `<Space>v` cycle layout, `<C-=>`/`<C-->` zoom,
`:pane-close` close pane, `:q` quit, `<F1>` all bindings.

### Installing on Linux

```sh
packaging/linux/install.sh            # builds cecilia into ~/.local/bin, with a desktop entry
sudo packaging/linux/install.sh /usr/local
```

This installs the binary, `cecilia.desktop` (which opens cecilia in a window with `--gui`), an icon,
and file types for `.cc`, Guitar Pro (`.gp`, `.gpx`, `.gp3`–`.gp5`) and `.gabc` files, so file
managers offer cecilia for them. `packaging/linux/uninstall.sh [PREFIX]` removes them again.

## Configuration

`~/.config/cecilia/config.toml` — see [docs/config.example.toml](docs/config.example.toml).
For realistic instrument sounds set `audio.soundfont` to a General MIDI `.sf2` file
(e.g. FluidR3_GM or GeneralUser GS); otherwise the built-in synth is used.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Format tests also parse every Guitar Pro file under `test-corpus/` (gitignored; point it at
your own collection, e.g. `ln -s ~/Music/Tabs test-corpus`).

## Fonts

Bundled under the SIL Open Font License 1.1 (texts in `assets/fonts/`): Bravura (music symbols),
Source Sans 3 (text) and Caeciliae by the Oblates of
St. Joseph (square chant notation).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
